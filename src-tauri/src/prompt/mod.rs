pub mod compressor;
pub mod config;

use rusqlite::params;
use rusqlite::Connection;
use serde::Serialize;
use sha2::{Digest, Sha256};
use tauri::State;

use crate::gateway::schema::{ChatReq, ToolCall, WireMsg};
use crate::gateway::{store as gw_store, Gateway};

pub const BASE: &str = "You are Argus, a personal AI agent operating on the user's machine.\n\
Rules:\n\
1. Act, don't just suggest. Say plainly what you did.\n\
2. Never invent file contents, command output, URLs, or tool results.\n\
3. Short answers unless depth is asked for.\n\
4. Permission modes: ask, never. In ask mode some tools pause for user approval first. \
A denied action stays denied: say what failed and what would fix it, never retry it.\n\
\n\
## Reply format\n\
1. Ordinary prose in short paragraphs separated by blank lines, 2 to 4 sentences each.\n\
2. Never use markdown: no **, no ##, no ---, no backtick fences.\n\
3. Use only these tags: <h2> for section headings, <h3> for sub-parts, <bold>, \
<italic>, <code>, <link href=\"url\">text</link>, <table> with <tr><th><td>, \
<warning severity=\"low|medium|high\"> for caveats and risks, <thinking> for reasoning \
you want visible (it renders collapsed).\n\
4. Summarize tool results in your own words; never paste raw tool output into the reply.\n\
\n\
A correct reply looks like:\n\
<h2>Summary</h2>\n\
One short paragraph here. A <bold>key point</bold> stays bold and <code>a_cmd</code> renders as code.\n\
A second paragraph, after a blank line.\n\
\n\
Wrong: <p>hello</p> or <strong>hi</strong> or <i>hi</i> — these show literally.\n\
Never invent tags (no <command>, <output>, <p>, <div>, <span>, <strong>, <b>, <i>, <em>, <u>, <a> or anything not listed above), never \
wrap the whole reply in a tag, never fake tool output. \
A tag you invent shows up as literal text.
";

fn stable_layer(conn: &Connection, web: bool) -> Result<String, String> {
    let mut s = String::from(BASE);
    s.push_str(&crate::tools::section(web));

    if let Some(rules) = gw_store::kv_get(conn, "preference_rules") {
        if !rules.trim().is_empty() {
            s.push_str("\n\n## User preferences\n");
            s.push_str(rules.trim());
        }
    }

    let mut stmt = conn
        .prepare("SELECT name, description FROM skills ORDER BY name")
        .map_err(|err| err.to_string())?;

    let rows = stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
        .map_err(|err| err.to_string())?;

    let skills: Vec<(String, String)> = rows
        .collect::<Result<Vec<_>, _>>()
        .map_err(|err| err.to_string())?;

    s.push_str(
        "\n\n## Skills\nSearch live with skill.search; read a body with skill.read when its entry looks relevant.\n",
    );

    for (name, desc) in &skills {
        s.push_str(&format!("- {name}: {desc}\n"));
    }

    s.push_str(
        "Save reusable wins with skill.create (kebab-case name, one-line description, body of When to use, Steps, Pitfalls): after a hard multi-step success, or anytime the user says remember this. Search first so you never duplicate.\n",
    );

    s.push_str(
        "A message starting with @createskill is a skill request: use any text after the tag as context, ask for whatever of name, description, and body is still missing, then skill.create.\n",
    );

    s.push_str(
        "\n\n## Memory\nSearch past context with memory.search before answering from history; read a hit with memory.read. Save durable facts, preferences, decisions and project state with memory.save (kind fact|preference|project|person|decision). Recall checks memories plus past messages, summaries and indexed files, then follows memory_links graph neighbors.\n",
    );

    if let Ok(mems) = crate::memory::store::list(conn, None, 5) {
        let mut shown = 0usize;
        for m in &mems {
            if shown >= 5 {
                break;
            }
            let snippet: String = m.content.chars().take(160).collect();
            s.push_str(&format!("- [{}:{}] {}\n", m.kind, m.id, snippet));
            shown += 1;
        }
    }

    Ok(s)
}

fn recall_terms(content: &str) -> Option<String> {
    const STOP: &[&str] = &[
        "with", "from", "that", "this", "what", "when", "about", "then", "than", "there", "their",
        "have", "will", "would", "could", "should", "your", "ours", "into",
    ];
    let terms: Vec<String> = content
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| t.len() >= 4)
        .map(|t| t.to_lowercase())
        .filter(|t| !STOP.contains(&t.as_str()))
        .take(8)
        .map(|t| format!("\"{}\"", t.replace('"', "")))
        .collect();
    if terms.is_empty() {
        return None;
    }

    Some(terms.join(" "))
}

fn session_start_memories(conn: &Connection, session_id: &str) -> Option<String> {
    let user_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM messages WHERE session_id = ?1 AND role = 'user' \
             AND active = 1 AND content NOT LIKE '<tool-result%'",
            params![session_id],
            |r| r.get(0),
        )
        .unwrap_or(0);
    if user_count != 1 {
        return None;
    }

    let query: String = conn
        .query_row(
            "SELECT content FROM messages WHERE session_id = ?1 AND role = 'user' \
             AND active = 1 AND content NOT LIKE '<tool-result%' \
             ORDER BY seq DESC LIMIT 1",
            params![session_id],
            |r| r.get(0),
        )
        .ok()?;
    let query = recall_terms(&query)?;
    let hits = crate::memory::store::recall(conn, &query, 8).ok()?;

    let mut out = String::from(
        "## Relevant memories from past sessions (use memory.read on an id for full detail)\n",
    );
    let mut shown = 0usize;
    for h in hits.iter().filter(|h| h.source != "message").take(4) {
        let snippet: String = h.snippet.chars().take(160).collect();
        out.push_str(&format!("- [{}:{}] {}\n", h.source, h.ref_id, snippet));
        shown += 1;
    }

    if shown == 0 {
        return None;
    }

    Some(out)
}

pub fn project(conn: &Connection, session_id: &str) -> Result<Projection, String> {
    let (model_id, compact_seq, ctx_tokens, web_search) = conn
        .query_row(
            "SELECT model_id, compact_seq, ctx_tokens, web_search FROM sessions WHERE id = ?1",
            params![session_id],
            |r| {
                Ok((
                    r.get::<_, Option<String>>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, i64>(2)?,
                    r.get::<_, i64>(3)? != 0,
                ))
            },
        )
        .map_err(|_| format!("session {session_id} not found"))?;

    let stable = stable_layer(conn, web_search)?;

    let summary: Option<String> = conn
        .query_row(
            "SELECT content FROM summaries WHERE session_id = ?1 ORDER BY created_at DESC LIMIT 1",
            params![session_id],
            |r| r.get(0),
        )
        .ok();

    let mut system = stable.clone();
    if let Some(sum) = &summary {
        system.push_str("\n\n## Earlier in this session (compacted)\n");
        system.push_str(sum);
    }

    if let Some(notes) = session_start_memories(conn, session_id) {
        system.push_str("\n\n");
        system.push_str(&notes);
    }

    if let Some(notes) = crate::tools::notepad::prompt_include(session_id) {
        system.push_str("\n\n");
        system.push_str(&notes);
    }

    let mut stmt = conn
        .prepare(
            "SELECT role, content, tool_calls, tool_call_id FROM messages
             WHERE session_id = ?1 AND active = 1 AND seq > ?2
             ORDER BY seq",
        )
        .map_err(|err| err.to_string())?;

    let rows = stmt
        .query_map(params![session_id, compact_seq], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, Option<String>>(3)?,
            ))
        })
        .map_err(|err| err.to_string())?;

    let rows = rows
        .collect::<Result<Vec<_>, _>>()
        .map_err(|err| err.to_string())?;

    let msgs = to_wire(&rows);

    let mut hasher = Sha256::new();
    hasher.update(system.as_bytes());
    let hash: String = hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();

    Ok(Projection {
        session_id: session_id.into(),
        model_id,
        system,
        summary,
        msgs,
        prefix_hash: hash,
        ctx_tokens,
        compact_seq,
        web: web_search,
    })
}

fn to_wire(rows: &[(String, String, Option<String>, Option<String>)]) -> Vec<WireMsg> {
    rows.iter()
        .map(|(role, content, calls, call_id)| {
            let calls = calls
                .as_deref()
                .and_then(|j| serde_json::from_str::<Vec<ToolCall>>(j).ok())
                .unwrap_or_default();

            if role == "user" && content.starts_with("<tool-result") {
                let has_id = call_id.as_deref().is_some_and(|s| !s.is_empty());
                if has_id {
                    return WireMsg {
                        role: "tool".into(),
                        content: unwrap_result(content),
                        images: vec![],
                        tool_calls: vec![],
                        tool_call_id: call_id.clone(),
                    };
                }

                return WireMsg {
                    role: "user".into(),
                    content: content.clone(),
                    images: vec![],
                    tool_calls: vec![],
                    tool_call_id: None,
                };
            }

            WireMsg {
                role: role.clone(),
                content: content.clone(),
                images: vec![],
                tool_calls: calls,
                tool_call_id: None,
            }
        })
        .collect()
}

fn unwrap_result(content: &str) -> String {
    let inner = match (content.find('>'), content.rfind("</tool-result>")) {
        (Some(open), Some(end)) if open + 1 <= end => &content[open + 1..end],
        _ => content,
    };

    let err = content.contains("status=\"err\"");

    if err {
        format!("error: {inner}")
    } else {
        inner.to_string()
    }
}

pub struct Projection {
    pub session_id: String,
    pub model_id: Option<String>,
    pub system: String,
    pub summary: Option<String>,
    pub msgs: Vec<WireMsg>,
    pub prefix_hash: String,
    pub ctx_tokens: i64,
    pub compact_seq: i64,
    pub web: bool,
}

impl Projection {
    pub fn chat_req(&self) -> ChatReq {
        let mut msgs = vec![WireMsg {
            role: "system".into(),
            content: self.system.clone(),
            ..Default::default()
        }];
        msgs.extend(self.msgs.iter().cloned());

        ChatReq {
            model: self.model_id.clone().unwrap_or_default(),
            msgs,
            prefix_hash: None,
            tools: crate::tools::tool_specs(self.web),
        }
    }
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct PromptPreview {
    pub session_id: String,
    pub model_id: Option<String>,
    pub system: String,
    pub summary: Option<String>,
    pub msgs: Vec<WireMsg>,
    pub prefix_hash: String,
    pub ctx_tokens: i64,
    pub compact_seq: i64,
}

#[tauri::command]
pub fn prompt_preview(gw: State<'_, Gateway>, session_id: String) -> Result<PromptPreview, String> {
    let conn = gw.conn.lock().map_err(|err| err.to_string())?;
    let p = project(&conn, &session_id)?;

    Ok(PromptPreview {
        session_id: p.session_id,
        model_id: p.model_id,
        system: p.system,
        summary: p.summary,
        msgs: p.msgs,
        prefix_hash: p.prefix_hash,
        ctx_tokens: p.ctx_tokens,
        compact_seq: p.compact_seq,
    })
}
