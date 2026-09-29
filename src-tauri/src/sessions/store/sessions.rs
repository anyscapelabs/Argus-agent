// Session rows: creation, listing, and the small column patches the UI makes.
use rusqlite::{params, Connection, OptionalExtension};
use uuid::Uuid;

use super::super::schema::{NewSession, Session};

const SESSION_COLS: &str = "id, title, status, model_id, permission, folder_id, \
                            created_at, updated_at, ctx_tokens, compact_seq, \
                            compactions, web_search, parent_id, agent_name, agent_state, \
                            profile_id, \
                            (SELECT COUNT(*) FROM sessions c \
                               WHERE c.parent_id = sessions.id \
                                 AND c.agent_state = 'running')";

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
        profile_id: r.get(15)?,
        running_agents: r.get(16)?,
    })
}

pub fn create_session(conn: &Connection, req: &NewSession) -> Result<Session, String> {
    let id = Uuid::new_v4().to_string();
    let perm = req.permission.clone().unwrap_or_else(|| "ask".into());

    conn.execute(
        "INSERT INTO sessions (id, title, model_id, permission, folder_id, web_search, profile_id) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'default')",
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

/// A null `profile_id` is the default profile, not one nobody owns.
pub fn list_profile_sessions(
    conn: &Connection,
    profile_id: &str,
    default_id: &str,
) -> Result<Vec<Session>, String> {
    let sql = format!(
        "SELECT {SESSION_COLS} FROM sessions \
         WHERE parent_id IS NULL AND IFNULL(profile_id, ?2) = ?1 \
         ORDER BY updated_at DESC"
    );

    let mut stmt = conn.prepare(&sql).map_err(|err| err.to_string())?;

    let rows = stmt
        .query_map(params![profile_id, default_id], row_session)
        .map_err(|err| err.to_string())?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|err| err.to_string())
}

/// Counts what `list_msgs` would hand back, so the two agree.
pub fn count_msgs(conn: &Connection, session_id: &str) -> Result<i64, String> {
    conn.query_row(
        "SELECT COUNT(*) FROM messages WHERE session_id = ?1 AND active = 1",
        params![session_id],
        |r| r.get(0),
    )
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

    // A sub-agent is its parent's, working. A Senior Developer that fans out
    // produces senior developers, so the profile comes along.
    conn.execute(
        "INSERT INTO sessions (id, title, model_id, permission, parent_id, agent_name, agent_state, profile_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'running',
                 COALESCE((SELECT profile_id FROM sessions WHERE id = ?5), 'default'))",
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
