use rusqlite::{params, Connection, OptionalExtension};
use uuid::Uuid;

use super::schema::{Folder, Msg, NewMsg, NewSession, Session};

pub fn migrate(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(super::schema::MIGRATE)
        .map_err(|err| err.to_string())?;

    let has_vote: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('messages') WHERE name = 'vote'",
            [],
            |r| r.get::<_, i64>(0),
        )
        .map(|n| n > 0)
        .map_err(|err| err.to_string())?;

    if !has_vote {
        conn.execute("ALTER TABLE messages ADD COLUMN vote TEXT", [])
            .map_err(|err| err.to_string())?;
    }

    let has_web: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('sessions') WHERE name = 'web_search'",
            [],
            |r| r.get::<_, i64>(0),
        )
        .map(|n| n > 0)
        .map_err(|err| err.to_string())?;

    if !has_web {
        conn.execute(
            "ALTER TABLE sessions ADD COLUMN web_search INTEGER NOT NULL DEFAULT 0",
            [],
        )
        .map_err(|err| err.to_string())?;
    }

    let has_reflect: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('sessions') WHERE name = 'reflect'",
            [],
            |r| r.get::<_, i64>(0),
        )
        .map(|n| n > 0)
        .map_err(|err| err.to_string())?;

    if !has_reflect {
        conn.execute(
            "ALTER TABLE sessions ADD COLUMN reflect INTEGER NOT NULL DEFAULT 0",
            [],
        )
        .map_err(|err| err.to_string())?;
    }

    let has_calls: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('messages') WHERE name = 'tool_calls'",
            [],
            |r| r.get::<_, i64>(0),
        )
        .map(|n| n > 0)
        .map_err(|err| err.to_string())?;

    if !has_calls {
        conn.execute("ALTER TABLE messages ADD COLUMN tool_calls TEXT", [])
            .map_err(|err| err.to_string())?;
    }

    let has_call_id: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('messages') WHERE name = 'tool_call_id'",
            [],
            |r| r.get::<_, i64>(0),
        )
        .map(|n| n > 0)
        .map_err(|err| err.to_string())?;

    if !has_call_id {
        conn.execute("ALTER TABLE messages ADD COLUMN tool_call_id TEXT", [])
            .map_err(|err| err.to_string())?;
    }

    let has_kind: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('messages') WHERE name = 'kind'",
            [],
            |r| r.get::<_, i64>(0),
        )
        .map(|n| n > 0)
        .map_err(|err| err.to_string())?;

    if !has_kind {
        conn.execute("ALTER TABLE messages ADD COLUMN kind TEXT", [])
            .map_err(|err| err.to_string())?;
    }

    for (col, ddl) in [
        (
            "parent_id",
            "ALTER TABLE sessions ADD COLUMN parent_id TEXT",
        ),
        (
            "agent_name",
            "ALTER TABLE sessions ADD COLUMN agent_name TEXT",
        ),
        (
            "agent_state",
            "ALTER TABLE sessions ADD COLUMN agent_state TEXT",
        ),
    ] {
        let has: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('sessions') WHERE name = ?1",
                params![col],
                |r| r.get::<_, i64>(0),
            )
            .map(|n| n > 0)
            .map_err(|err| err.to_string())?;

        if !has {
            conn.execute(ddl, []).map_err(|err| err.to_string())?;
        }
    }

    conn.pragma_update(None, "foreign_keys", true)
        .map_err(|err| err.to_string())?;

    Ok(())
}

const SESSION_COLS: &str = "id, title, status, model_id, permission, folder_id, \
                            created_at, updated_at, ctx_tokens, compact_seq, \
                            compactions, web_search, parent_id, agent_name, agent_state";

fn row_session(r: &rusqlite::Row) -> rusqlite::Result<Session> {
    Ok(Session {
        id: r.get(0)?,
        title: r.get(1)?,
        status: r.get(2)?,
        model_id: r.get(3)?,
        permission: r.get(4)?,
        folder_id: r.get(5)?,
        created_at: r.get(6)?,
        updated_at: r.get(7)?,
        ctx_tokens: r.get(8)?,
        compact_seq: r.get(9)?,
        compactions: r.get(10)?,
        web_search: r.get::<_, i64>(11)? != 0,
        parent_id: r.get(12)?,
        agent_name: r.get(13)?,
        agent_state: r.get(14)?,
    })
}

pub fn create_session(conn: &Connection, req: &NewSession) -> Result<Session, String> {
    let id = Uuid::new_v4().to_string();
    let perm = req.permission.clone().unwrap_or_else(|| "ask".into());

    conn.execute(
        "INSERT INTO sessions (id, title, model_id, permission, folder_id, web_search) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![id, req.title, req.model_id, perm, req.folder_id, req.web_search],
    )
    .map_err(|err| err.to_string())?;

    get_session(conn, &id)
}

pub fn get_session(conn: &Connection, id: &str) -> Result<Session, String> {
    conn.query_row(
        &format!("SELECT {SESSION_COLS} FROM sessions WHERE id = ?1"),
        params![id],
        row_session,
    )
    .optional()
    .map_err(|err| err.to_string())?
    .ok_or_else(|| "session not found".into())
}

pub fn list_sessions(conn: &Connection, folder_id: Option<&str>) -> Result<Vec<Session>, String> {
    // Children belong to their parent, not to the user's list of chats.
    let sql = match folder_id {
        Some(_) => format!(
            "SELECT {SESSION_COLS} FROM sessions WHERE parent_id IS NULL AND folder_id = ?1 \
             ORDER BY updated_at DESC"
        ),
        None => format!(
            "SELECT {SESSION_COLS} FROM sessions WHERE parent_id IS NULL ORDER BY updated_at DESC"
        ),
    };

    let mut stmt = conn.prepare(&sql).map_err(|err| err.to_string())?;

    let rows = match folder_id {
        Some(fid) => stmt.query_map(params![fid], row_session),
        None => stmt.query_map([], row_session),
    }
    .map_err(|err| err.to_string())?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|err| err.to_string())
}

pub fn is_child(conn: &Connection, session_id: &str) -> Result<bool, String> {
    conn.query_row(
        "SELECT parent_id IS NOT NULL FROM sessions WHERE id = ?1",
        params![session_id],
        |r| r.get(0),
    )
    .map_err(|err| err.to_string())
}

pub fn children_of(conn: &Connection, parent_id: &str) -> Result<Vec<Session>, String> {
    let mut stmt = conn
        .prepare(&format!(
            "SELECT {SESSION_COLS} FROM sessions WHERE parent_id = ?1 ORDER BY created_at"
        ))
        .map_err(|err| err.to_string())?;

    let rows = stmt
        .query_map(params![parent_id], row_session)
        .map_err(|err| err.to_string())?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|err| err.to_string())
}

pub fn set_agent_state(
    conn: &Connection,
    session_id: &str,
    state: &str,
    title: Option<&str>,
) -> Result<(), String> {
    match title {
        Some(t) => conn.execute(
            "UPDATE sessions SET agent_state = ?2, title = ?3, updated_at = datetime('now')
             WHERE id = ?1",
            params![session_id, state, t],
        ),
        None => conn.execute(
            "UPDATE sessions SET agent_state = ?2, updated_at = datetime('now') WHERE id = ?1",
            params![session_id, state],
        ),
    }
    .map_err(|err| err.to_string())?;

    Ok(())
}

pub fn create_child(
    conn: &Connection,
    parent_id: &str,
    name: &str,
    title: &str,
    model_id: Option<&str>,
    permission: &str,
) -> Result<Session, String> {
    let id = Uuid::new_v4().to_string();

    conn.execute(
        "INSERT INTO sessions (id, title, model_id, permission, parent_id, agent_name, agent_state)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'running')",
        params![id, title, model_id, permission, parent_id, name],
    )
    .map_err(|err| err.to_string())?;

    get_session(conn, &id)
}

pub fn save_session(conn: &Connection, s: &Session) -> Result<(), String> {
    conn.execute(
        "UPDATE sessions SET title=?2, status=?3, model_id=?4, permission=?5, folder_id=?6,
         ctx_tokens=?7, compact_seq=?8, compactions=?9, web_search=?10, updated_at=datetime('now') WHERE id=?1",
        params![
            s.id,
            s.title,
            s.status,
            s.model_id,
            s.permission,
            s.folder_id,
            s.ctx_tokens,
            s.compact_seq,
            s.compactions,
            s.web_search
        ],
    )
    .map_err(|err| err.to_string())?;

    Ok(())
}

pub fn touch_session(conn: &Connection, id: &str, ctx_tokens: i64) -> Result<(), String> {
    conn.execute(
        "UPDATE sessions SET ctx_tokens=?2, updated_at=datetime('now') WHERE id=?1",
        params![id, ctx_tokens],
    )
    .map_err(|err| err.to_string())?;

    Ok(())
}

pub fn set_permission(conn: &Connection, id: &str, permission: &str) -> Result<(), String> {
    conn.execute(
        "UPDATE sessions SET permission=?2, updated_at=datetime('now') WHERE id=?1",
        params![id, permission],
    )
    .map_err(|err| err.to_string())?;

    Ok(())
}

pub fn set_reflect(conn: &Connection, id: &str, on: bool) -> Result<(), String> {
    conn.execute(
        "UPDATE sessions SET reflect=?2, updated_at=datetime('now') WHERE id=?1",
        params![id, on as i64],
    )
    .map_err(|err| err.to_string())?;

    Ok(())
}

pub fn set_model(conn: &Connection, id: &str, model_id: Option<&str>) -> Result<(), String> {
    conn.execute(
        "UPDATE sessions SET model_id=?2, updated_at=datetime('now') WHERE id=?1",
        params![id, model_id],
    )
    .map_err(|err| err.to_string())?;

    Ok(())
}

pub fn set_web_search(conn: &Connection, id: &str, on: bool) -> Result<(), String> {
    conn.execute(
        "UPDATE sessions SET web_search=?2, updated_at=datetime('now') WHERE id=?1",
        params![id, on],
    )
    .map_err(|err| err.to_string())?;

    Ok(())
}

pub fn delete_session(conn: &Connection, id: &str) -> Result<(), String> {
    for sql in [
        "DELETE FROM messages WHERE session_id = ?1",
        "DELETE FROM summaries WHERE session_id = ?1",
        "DELETE FROM sessions WHERE id = ?1",
    ] {
        conn.execute(sql, params![id])
            .map_err(|err| err.to_string())?;
    }

    Ok(())
}

const MSG_COLS: &str =
    "id, session_id, seq, role, content, model_id, provider_id, tok_in, tok_out, active, vote, tool_calls, tool_call_id, kind, created_at";

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

pub fn add_msg(conn: &Connection, m: &NewMsg) -> Result<Msg, String> {
    let id = Uuid::new_v4().to_string();
    let seq: i64 = conn
        .query_row(
            "SELECT COALESCE(MAX(seq), 0) + 1 FROM messages WHERE session_id = ?1",
            params![m.session_id],
            |r| r.get(0),
        )
        .map_err(|err| err.to_string())?;

    conn.execute(
        "INSERT INTO messages (id, session_id, seq, role, content, model_id, provider_id, tok_in, tok_out, tool_calls, tool_call_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
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
            m.tool_call_id
        ],
    )
    .map_err(|err| err.to_string())?;

    get_msg(conn, &id)
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
    conn.execute(
        "UPDATE messages SET active = 0 \
         WHERE session_id = ?1 AND role = 'user' AND active = 1 \
         AND NOT EXISTS (\
           SELECT 1 FROM messages a \
           WHERE a.session_id = messages.session_id AND a.seq > messages.seq \
           AND a.role = 'assistant' AND a.active = 1)",
        params![session_id],
    )
    .map_err(|err| err.to_string())
}

pub fn create_folder(conn: &Connection, name: &str) -> Result<Folder, String> {
    let id = Uuid::new_v4().to_string();

    conn.execute(
        "INSERT INTO folders (id, name) VALUES (?1, ?2)",
        params![id, name],
    )
    .map_err(|err| err.to_string())?;

    conn.query_row(
        "SELECT id, name, created_at FROM folders WHERE id = ?1",
        params![id],
        |r| {
            Ok(Folder {
                id: r.get(0)?,
                name: r.get(1)?,
                created_at: r.get(2)?,
            })
        },
    )
    .map_err(|err| err.to_string())
}

pub fn list_folders(conn: &Connection) -> Result<Vec<Folder>, String> {
    let mut stmt = conn
        .prepare("SELECT id, name, created_at FROM folders ORDER BY created_at")
        .map_err(|err| err.to_string())?;

    let rows = stmt
        .query_map([], |r| {
            Ok(Folder {
                id: r.get(0)?,
                name: r.get(1)?,
                created_at: r.get(2)?,
            })
        })
        .map_err(|err| err.to_string())?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|err| err.to_string())
}

pub fn export_json(conn: &Connection, id: &str) -> Result<String, String> {
    let session = get_session(conn, id)?;

    let mut stmt = conn
        .prepare(
            "SELECT role, content FROM messages WHERE session_id = ?1 AND active = 1 ORDER BY seq",
        )
        .map_err(|err| err.to_string())?;

    let rows = stmt
        .query_map(params![id], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })
        .map_err(|err| err.to_string())?;

    let msgs = rows
        .collect::<Result<Vec<_>, _>>()
        .map_err(|err| err.to_string())?;

    let messages: Vec<serde_json::Value> = msgs
        .into_iter()
        .map(|(role, content)| {
            let key = if role == "user" { "user" } else { "agent" };
            serde_json::json!({ key: content })
        })
        .collect();

    serde_json::to_string_pretty(&serde_json::json!({
        "title": session.title,
        "model": session.model_id,
        "messages": messages,
    }))
    .map_err(|err| err.to_string())
}
