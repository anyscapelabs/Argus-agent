pub mod compressor;
pub mod config;

use rusqlite::params;
use rusqlite::Connection;
use serde::Serialize;
use sha2::{Digest, Sha256};
use tauri::State;

use crate::gateway::schema::{ChatReq, WireMsg};
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

    if !skills.is_empty() {
        s.push_str(
            "\n\n## Skills\nRead a skill's full body with skill.read when its index entry looks relevant; the index below is all you get by default.\n",
        );
        for (name, desc) in skills {
            s.push_str(&format!("- {name}: {desc}\n"));
        }
    }

    Ok(s)
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

    let mut stmt = conn
        .prepare(
            "SELECT role, content FROM messages
             WHERE session_id = ?1 AND active = 1 AND seq > ?2
             ORDER BY seq",
        )
        .map_err(|err| err.to_string())?;

    let rows = stmt
        .query_map(params![session_id, compact_seq], |r| {
            Ok(WireMsg {
                role: r.get(0)?,
                content: r.get(1)?,
                images: vec![],
            })
        })
        .map_err(|err| err.to_string())?;

    let msgs = rows
        .collect::<Result<Vec<_>, _>>()
        .map_err(|err| err.to_string())?;

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
    })
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
}

impl Projection {
    pub fn chat_req(&self) -> ChatReq {
        let mut msgs = vec![WireMsg {
            role: "system".into(),
            content: self.system.clone(),
            images: vec![],
        }];
        msgs.extend(self.msgs.iter().cloned());

        ChatReq {
            model: self.model_id.clone().unwrap_or_default(),
            msgs,
            prefix_hash: None,
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
