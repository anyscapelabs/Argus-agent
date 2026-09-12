use rusqlite::{params, Connection};
use serde::Serialize;
use tauri::State;
use uuid::Uuid;

use super::config::{context_window, est_tokens, truncate_chars, CompressionCfg};
use super::project;
use crate::gateway::router;
use crate::gateway::schema::{ChatReq, WireMsg};
use crate::gateway::Gateway;

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ContextStatus {
    pub tok_in: i64,
    pub est_tok_in: i64,
    pub context: i64,
    pub ratio: f64,
    pub est_ratio: f64,
    pub threshold: f64,
    pub needs_compact: bool,
}

pub fn check(conn: &Connection, session_id: &str) -> Result<ContextStatus, String> {
    let (model_id, ctx_tokens) = conn
        .query_row(
            "SELECT model_id, ctx_tokens FROM sessions WHERE id = ?1",
            params![session_id],
            |r| Ok((r.get::<_, Option<String>>(0)?, r.get::<_, i64>(1)?)),
        )
        .map_err(|_| format!("session {session_id} not found"))?;

    let cfg = CompressionCfg::load(conn);
    let context = context_window(conn, model_id.as_deref());
    let p = project(conn, session_id)?;

    let mut est = est_tokens(&p.system);
    for m in &p.msgs {
        est += est_tokens(&m.content);
    }

    let real = if ctx_tokens > 0 { ctx_tokens } else { est };
    let threshold = cfg.threshold_for(model_id.as_deref().unwrap_or(""));
    let ratio = real as f64 / context.max(1) as f64;
    let est_ratio = est as f64 / context.max(1) as f64;

    Ok(ContextStatus {
        tok_in: real,
        est_tok_in: est,
        context,
        ratio,
        est_ratio,
        threshold,
        needs_compact: ratio >= threshold || est_ratio >= cfg.safety,
    })
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct CompactionRecord {
    pub covers_to: i64,
    pub compact_seq: i64,
    pub tok_before: i64,
    pub summary_model: String,
}

const SUMMARY_SYS: &str = "You compress conversation history. Output only the summary: no code fences, no extra text. Follow this template exactly:\n\
## Goal\n## Constraints & Preferences\n## Progress\n### Done\n### In Progress\n### Blocked\n\
## Key Decisions\n## Relevant Files\n## Next Steps\n## Critical Context\n\
Keep every heading; write `none` under a heading with no content. Omit greetings and small talk. \
Preserve exact file paths, commands, names, and numbers. Only use facts from the transcript — never invent paths, commands, or outcomes. Stay under the stated token budget.";

pub fn utility_model(conn: &Connection) -> Result<String, String> {
    conn.query_row(
        "SELECT m.id FROM models m
         JOIN model_providers mp ON mp.model_id = m.id
         JOIN providers p ON p.id = mp.provider_id
         WHERE p.connected = 1 AND m.enabled = 1
         ORDER BY p.free DESC, mp.cost_in ASC, mp.cost_out ASC
         LIMIT 1",
        [],
        |r| r.get::<_, String>(0),
    )
    .map_err(|_| "no enabled model available for compaction".to_string())
}

pub async fn compact(gw: &Gateway, session_id: &str) -> Result<Option<CompactionRecord>, String> {
    let (cfg, model_id, _compact_seq, prev_summary, window) = {
        let conn = gw.conn.lock().map_err(|err| err.to_string())?;
        let cfg = CompressionCfg::load(&conn);
        let (model_id, compact_seq) = conn
            .query_row(
                "SELECT model_id, compact_seq FROM sessions WHERE id = ?1",
                params![session_id],
                |r| Ok((r.get::<_, Option<String>>(0)?, r.get::<_, i64>(1)?)),
            )
            .map_err(|_| format!("session {session_id} not found"))?;

        let prev_summary: Option<String> = conn
            .query_row(
                "SELECT content FROM summaries WHERE session_id = ?1 ORDER BY created_at DESC LIMIT 1",
                params![session_id],
                |r| r.get(0),
            )
            .ok();

        let mut stmt = conn
            .prepare(
                "SELECT seq, role, content FROM messages
                 WHERE session_id = ?1 AND active = 1 AND seq > ?2
                 ORDER BY seq",
            )
            .map_err(|err| err.to_string())?;

        let rows = stmt
            .query_map(params![session_id, compact_seq], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })
            .map_err(|err| err.to_string())?;

        let window = rows
            .collect::<Result<Vec<_>, _>>()
            .map_err(|err| err.to_string())?;
        (cfg, model_id, compact_seq, prev_summary, window)
    };

    if !cfg.enabled {
        return Ok(None);
    }

    let context = {
        let conn = gw.conn.lock().map_err(|err| err.to_string())?;
        context_window(&conn, model_id.as_deref())
    };

    let protect_last = cfg.protect_last.max(1) as usize;
    let protect_first = cfg.protect_first.max(0) as usize;

    if window.len() <= protect_first + protect_last {
        return Ok(None);
    }

    let tail_budget = (context as f64
        * cfg.threshold_for(model_id.as_deref().unwrap_or(""))
        * cfg.target_ratio) as i64;

    let mut acc = 0i64;
    let mut tail_start = window.len();

    for (i, (_, _, content)) in window.iter().enumerate().rev() {
        let remaining = window.len() - i;
        if remaining < protect_last {
            tail_start = i;
            continue;
        }

        acc += est_tokens(content);
        if acc >= tail_budget {
            break;
        }

        tail_start = i;
    }

    let head_end = protect_first.min(tail_start);
    let middle = &window[head_end..tail_start];

    let mut middle_tokens = 0i64;
    for (_, _, content) in middle {
        middle_tokens += est_tokens(content);
    }

    if middle.is_empty() || middle_tokens < 400 {
        return Ok(None);
    }

    let util = {
        let conn = gw.conn.lock().map_err(|err| err.to_string())?;
        utility_model(&conn)?
    };

    let budget = (middle_tokens * 2 / 10).clamp(200, 2000);
    let words = budget * 3 / 4;

    let mut transcript = String::new();
    for (seq, role, content) in middle {
        transcript.push_str(&format!(
            "[{seq}] {role}: {}\n",
            truncate_chars(content, 1000)
        ));
    }

    let user = match &prev_summary {
        Some(prev) => format!(
            "An earlier summary of this conversation follows. Update it in place with the new transcript: \
move finished items from In Progress to Done, add new decisions and files, drop stale ones. \
Do not start from scratch. Stay under {budget} tokens (about {words} words).\n\n\
<previous_summary>\n{prev}\n</previous_summary>\n\n<new_transcript>\n{transcript}\n</new_transcript>"
        ),
        None => format!(
            "Summarize the conversation transcript below for a continuation agent that will read only \
this summary plus the most recent messages. Stay under {budget} tokens (about {words} words).\n\n\
<transcript>\n{transcript}\n</transcript>"
        ),
    };

    let req = ChatReq {
        model: util.clone(),
        msgs: vec![
            WireMsg {
                role: "system".into(),
                content: SUMMARY_SYS.into(),
                images: vec![],
            },
            WireMsg {
                role: "user".into(),
                content: user,
                images: vec![],
            },
        ],
        prefix_hash: None,
    };

    let resp = router::run(gw, &req).await?;
    let summary = resp.content.trim().to_string();

    if summary.is_empty() {
        return Err("compaction model returned an empty summary".into());
    }

    let covers_to = middle[middle.len() - 1].0;
    let tok_before = middle_tokens;

    {
        let conn = gw.conn.lock().map_err(|err| err.to_string())?;
        conn.execute(
            "INSERT INTO summaries (id, session_id, covers_to, content, model_id) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![Uuid::new_v4().to_string(), session_id, covers_to, summary, util],
        )
        .map_err(|err| err.to_string())?;

        conn.execute(
            "UPDATE sessions SET compact_seq = ?2, compactions = compactions + 1 WHERE id = ?1",
            params![session_id, covers_to],
        )
        .map_err(|err| err.to_string())?;
    }

    Ok(Some(CompactionRecord {
        covers_to,
        compact_seq: covers_to,
        tok_before,
        summary_model: util,
    }))
}

#[tauri::command]
pub fn prompt_status(gw: State<'_, Gateway>, session_id: String) -> Result<ContextStatus, String> {
    let conn = gw.conn.lock().map_err(|err| err.to_string())?;
    check(&conn, &session_id)
}

#[tauri::command]
pub async fn prompt_compact(
    gw: State<'_, Gateway>,
    session_id: String,
) -> Result<Option<CompactionRecord>, String> {
    compact(&gw, &session_id).await
}
