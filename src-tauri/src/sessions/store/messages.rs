// Message rows: the transcript.
use rusqlite::{params, Connection, OptionalExtension};
use uuid::Uuid;

use super::super::schema::{Msg, NewMsg};

const MSG_COLS: &str =
    "id, session_id, seq, role, content, model_id, provider_id, tok_in, tok_out, active, vote, tool_calls, tool_call_id, kind, created_at, attachments, local";

fn row_msg(r: &rusqlite::Row) -> rusqlite::Result<Msg> {
    Ok(Msg {
        id: r.get(0)?,
        session_id: r.get(1)?,
        seq: r.get(2)?,
        role: r.get(3)?,
        content: r.get(4)?,
        model_id: r.get(5)?,
        provider_id: r.get(6)?,
        tok_in: r.get(7)?,
        tok_out: r.get(8)?,
        active: r.get::<_, i64>(9)? != 0,
        vote: r.get(10)?,
        tool_calls: r.get(11)?,
        tool_call_id: r.get(12)?,
        kind: r.get(13)?,
        created_at: r.get(14)?,
        attachments: r.get(15)?,
        local: r.get::<_, i64>(16)? != 0,
    })
}

pub fn mark_final(conn: &Connection, id: &str) -> Result<(), String> {
    conn.execute(
        "UPDATE messages SET kind = 'final' WHERE id = ?1",
        params![id],
    )
    .map_err(|err| err.to_string())?;

    Ok(())
}

// The newest assistant turn, closed or not: a budget stop writes its notice
// onto whatever the user last saw.
pub fn get_last_final(conn: &Connection, session_id: &str) -> Result<String, String> {
    conn.query_row(
        "SELECT id FROM messages
         WHERE session_id = ?1 AND role = 'assistant' AND active = 1
         ORDER BY seq DESC LIMIT 1",
        params![session_id],
        |r| r.get(0),
    )
    .map_err(|err| err.to_string())
}

/// The task as first asked. Raw content, not a rendered line: the transcript
/// strips tags.
pub fn first_user_msg(conn: &Connection, session_id: &str) -> Option<String> {
    conn.query_row(
        "SELECT content FROM messages
         WHERE session_id = ?1 AND role = 'user' AND local = 0 AND active = 1
         ORDER BY seq LIMIT 1",
        params![session_id],
        |r| r.get(0),
    )
    .optional()
    .ok()
    .flatten()
    .filter(|c: &String| !c.trim().is_empty())
}

pub fn add_msg(conn: &Connection, m: &NewMsg) -> Result<Msg, String> {
    insert(conn, m, false)
}

/// Set here and nowhere else: the frontend cannot mark a message of its own
/// invisible.
pub fn add_local_msg(conn: &Connection, session_id: &str, content: &str) -> Result<Msg, String> {
    let m = NewMsg {
        session_id: session_id.into(),
        role: "user".into(),
        content: content.into(),
        ..Default::default()
    };

    insert(conn, &m, true)
}

fn insert(conn: &Connection, m: &NewMsg, local: bool) -> Result<Msg, String> {
    let id = Uuid::new_v4().to_string();
    let seq: i64 = conn
        .query_row(
            "SELECT COALESCE(MAX(seq), 0) + 1 FROM messages WHERE session_id = ?1",
            params![m.session_id],
            |r| r.get(0),
        )
        .map_err(|err| err.to_string())?;

    conn.execute(
        "INSERT INTO messages (id, session_id, seq, role, content, model_id, provider_id, tok_in, tok_out, tool_calls, tool_call_id, attachments, local)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
        params![
            id,
            m.session_id,
            seq,
            m.role,
            m.content,
            m.model_id,
            m.provider_id,
            m.tok_in,
            m.tok_out,
            m.tool_calls,
            m.tool_call_id,
            m.attachments,
            local
        ],
    )
    .map_err(|err| err.to_string())?;

    get_msg(conn, &id)
}

/// A nudge rides the request, not the transcript, so a model answering the same
/// way twice said one thing twice. Overwrite the row; do not stack a copy.
pub fn add_msg_dedup(conn: &Connection, m: &NewMsg) -> Result<(Msg, bool), String> {
    if m.role != "assistant" {
        return add_msg(conn, m).map(|msg| (msg, false));
    }

    let last: Option<(String, String, Option<String>)> = conn
        .query_row(
            "SELECT id, role, kind FROM messages WHERE session_id = ?1 ORDER BY seq DESC LIMIT 1",
            params![m.session_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .ok();

    let Some((id, role, kind)) = last else {
        return add_msg(conn, m).map(|msg| (msg, false));
    };

    if role != "assistant" || kind.is_some() {
        return add_msg(conn, m).map(|msg| (msg, false));
    }

    let same: bool = conn
        .query_row(
            "SELECT content = ?2 FROM messages WHERE id = ?1",
            params![id, m.content],
            |r| r.get(0),
        )
        .unwrap_or(false);

    if !same {
        return add_msg(conn, m).map(|msg| (msg, false));
    }

    get_msg(conn, &id).map(|msg| (msg, true))
}

pub fn get_msg(conn: &Connection, id: &str) -> Result<Msg, String> {
    conn.query_row(
        &format!("SELECT {MSG_COLS} FROM messages WHERE id = ?1"),
        params![id],
        row_msg,
    )
    .optional()
    .map_err(|err| err.to_string())?
    .ok_or_else(|| "msg not found".into())
}

pub fn list_msgs(conn: &Connection, session_id: &str) -> Result<Vec<Msg>, String> {
    let mut stmt = conn
        .prepare(&format!(
            "SELECT {MSG_COLS} FROM messages WHERE session_id = ?1 AND active = 1 ORDER BY seq"
        ))
        .map_err(|err| err.to_string())?;

    let rows = stmt
        .query_map(params![session_id], row_msg)
        .map_err(|err| err.to_string())?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|err| err.to_string())
}
