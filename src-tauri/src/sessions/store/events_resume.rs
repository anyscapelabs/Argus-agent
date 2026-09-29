// Structured tool records, the resume slot, votes, supersede, and the
// dangling-result cleanup. These all touch rows that live next to messages.
use rusqlite::{params, Connection, OptionalExtension};
use uuid::Uuid;

use super::super::schema::{NewEvent, ResumeRow, ToolEvent};

const EVENT_COLS: &str =
    "id, message_id, session_id, kind, tool, args_json, status, elapsed_ms, code, output, label, detail, created_at";

fn row_event(r: &rusqlite::Row) -> rusqlite::Result<ToolEvent> {
    Ok(ToolEvent {
        id: r.get(0)?,
        message_id: r.get(1)?,
        session_id: r.get(2)?,
        kind: r.get(3)?,
        tool: r.get(4)?,
        args_json: r.get(5)?,
        status: r.get(6)?,
        elapsed_ms: r.get(7)?,
        code: r.get(8)?,
        output: r.get(9)?,
        label: r.get(10)?,
        detail: r.get(11)?,
        created_at: r.get(12)?,
    })
}

pub fn add_event(conn: &Connection, e: &NewEvent) -> Result<ToolEvent, String> {
    let id = Uuid::new_v4().to_string();

    conn.execute(
        "INSERT INTO tool_events (id, message_id, session_id, kind, tool, args_json, status, elapsed_ms, code, output, label, detail)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        params![
            id,
            e.message_id,
            e.session_id,
            e.kind,
            e.tool,
            e.args_json,
            e.status,
            e.elapsed_ms,
            e.code,
            e.output,
            e.label,
            e.detail,
        ],
    )
    .map_err(|err| err.to_string())?;

    conn.query_row(
        &format!("SELECT {EVENT_COLS} FROM tool_events WHERE id = ?1"),
        params![id],
        row_event,
    )
    .map_err(|err| err.to_string())
}

// Every event for the session, oldest first. Keyed by session rather than
// liveness: superseding a message retries the turn, it does not un-run tools.
pub fn list_events(conn: &Connection, session_id: &str) -> Result<Vec<ToolEvent>, String> {
    let mut stmt = conn
        .prepare(&format!(
            "SELECT {EVENT_COLS} FROM tool_events WHERE session_id = ?1 ORDER BY created_at, id"
        ))
        .map_err(|err| err.to_string())?;

    let rows = stmt
        .query_map(params![session_id], row_event)
        .map_err(|err| err.to_string())?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|err| err.to_string())
}

pub fn save_resume(conn: &Connection, session_id: &str, row: &ResumeRow) -> Result<(), String> {
    conn.execute(
        "INSERT INTO turn_resume (session_id, goal, done, next, updated_at)
         VALUES (?1, ?2, ?3, ?4, datetime('now'))
         ON CONFLICT(session_id) DO UPDATE SET goal = ?2, done = ?3, next = ?4, updated_at = datetime('now')",
        params![session_id, row.goal, row.done, row.next],
    )
    .map_err(|err| err.to_string())?;

    Ok(())
}

pub fn get_resume(conn: &Connection, session_id: &str) -> Option<ResumeRow> {
    conn.query_row(
        "SELECT goal, done, next FROM turn_resume WHERE session_id = ?1",
        params![session_id],
        |r| {
            Ok(ResumeRow {
                goal: r.get(0)?,
                done: r.get(1)?,
                next: r.get(2)?,
            })
        },
    )
    .optional()
    .ok()
    .flatten()
}

pub fn clear_resume(conn: &Connection, session_id: &str) -> Result<(), String> {
    conn.execute(
        "DELETE FROM turn_resume WHERE session_id = ?1",
        params![session_id],
    )
    .map_err(|err| err.to_string())?;

    Ok(())
}

pub fn set_vote(
    conn: &Connection,
    session_id: &str,
    msg_id: &str,
    vote: Option<&str>,
) -> Result<(), String> {
    conn.execute(
        "UPDATE messages SET vote = ?3 WHERE id = ?2 AND session_id = ?1",
        params![session_id, msg_id, vote],
    )
    .map_err(|err| err.to_string())?;

    Ok(())
}

pub fn supersede_from(conn: &Connection, session_id: &str, seq: i64) -> Result<(), String> {
    conn.execute(
        "UPDATE messages SET active = 0 WHERE session_id = ?1 AND seq >= ?2",
        params![session_id, seq],
    )
    .map_err(|err| err.to_string())?;

    Ok(())
}

pub fn has_unreplied_duplicate(
    conn: &Connection,
    session_id: &str,
    content: &str,
) -> Result<bool, String> {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return Ok(false);
    }

    let hit: Option<String> = conn
        .query_row(
            "SELECT content FROM messages WHERE session_id = ?1 AND active = 1 \
             AND role = 'user' AND content NOT LIKE '<tool-result%' \
             AND TRIM(content) = TRIM(?2) \
             AND created_at > datetime('now', '-60 seconds') \
             AND seq = (SELECT MAX(seq) FROM messages WHERE session_id = ?1)",
            params![session_id, trimmed],
            |r| r.get(0),
        )
        .optional()
        .map_err(|err| err.to_string())?;

    Ok(hit.is_some())
}

pub fn clean_dangling(conn: &Connection, session_id: &str) -> Result<usize, String> {
    // `local = 0` or the card a local row holds is greyed out the next time
    // the chat is opened: nothing ever answers a line the app answered itself,
    // which is exactly the shape this query exists to catch.
    conn.execute(
        "UPDATE messages SET active = 0 \
         WHERE session_id = ?1 AND role = 'user' AND active = 1 AND local = 0 \
         AND NOT EXISTS (\
           SELECT 1 FROM messages a \
           WHERE a.session_id = messages.session_id AND a.seq > messages.seq \
           AND a.role = 'assistant' AND a.active = 1)",
        params![session_id],
    )
    .map_err(|err| err.to_string())
}
