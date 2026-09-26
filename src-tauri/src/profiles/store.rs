use rusqlite::{params, Connection, OptionalExtension};
use uuid::Uuid;

use super::schema::{Profile, DEFAULT_ID, MAX_PROFILES, NAME_MAX};

fn row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Profile> {
    Ok(Profile {
        id: r.get(0)?,
        name: r.get(1)?,
        instructions: r.get::<_, Option<String>>(2)?.unwrap_or_default(),
        created_at: r.get(3)?,
    })
}

/// The default first, then newest. A profile with no name reads as "Default"
/// in the UI rather than as a blank row, so it belongs at the top where it is
/// the answer to "which one am I on".
pub fn list(conn: &Connection) -> Result<Vec<Profile>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, name, instructions, created_at FROM agent_profiles \
             ORDER BY (id = 'default') DESC, created_at DESC, rowid DESC",
        )
        .map_err(|err| err.to_string())?;

    let rows = stmt
        .query_map([], row)
        .map_err(|err| err.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|err| err.to_string())?;

    Ok(rows)
}

pub fn get(conn: &Connection, id: &str) -> Result<Profile, String> {
    conn.query_row(
        "SELECT id, name, instructions, created_at FROM agent_profiles WHERE id = ?1",
        params![id],
        row,
    )
    .optional()
    .map_err(|err| err.to_string())?
    .ok_or_else(|| "profile not found".into())
}

fn count(conn: &Connection) -> Result<i64, String> {
    conn.query_row("SELECT COUNT(*) FROM agent_profiles", [], |r| r.get(0))
        .map_err(|err| err.to_string())
}

pub fn create(conn: &Connection, name: &str) -> Result<Profile, String> {
    if count(conn)? as usize >= MAX_PROFILES {
        return Err(format!(
            "{MAX_PROFILES} profiles is the cap — delete one before adding another"
        ));
    }

    let id = Uuid::new_v4().to_string();
    let name = name.trim().chars().take(NAME_MAX).collect::<String>();

    conn.execute(
        "INSERT INTO agent_profiles (id, name) VALUES (?1, ?2)",
        params![id, name],
    )
    .map_err(|err| err.to_string())?;

    get(conn, &id)
}

pub fn edit(
    conn: &Connection,
    id: &str,
    name: Option<&str>,
    instructions: Option<&str>,
) -> Result<Profile, String> {
    // Loud on a profile that is not there, rather than a silent no-op that
    // reads in the UI as "saved".
    get(conn, id)?;

    if let Some(n) = name {
        let n = n.trim().chars().take(NAME_MAX).collect::<String>();

        conn.execute(
            "UPDATE agent_profiles SET name = ?2 WHERE id = ?1",
            params![id, n],
        )
        .map_err(|err| err.to_string())?;
    }

    if let Some(i) = instructions {
        conn.execute(
            "UPDATE agent_profiles SET instructions = ?2 WHERE id = ?1",
            params![id, i.trim()],
        )
        .map_err(|err| err.to_string())?;
    }

    get(conn, id)
}

pub fn delete(conn: &Connection, id: &str) -> Result<(), String> {
    if id == DEFAULT_ID {
        return Err("the default profile cannot be deleted".into());
    }

    // Refuse while it still owns work, and say which work, so the user is not
    // left wondering what happened to the chat they just opened.
    let owners: Vec<String> = {
        let mut stmt = conn
            .prepare("SELECT id FROM sessions WHERE profile_id = ?1 LIMIT 5")
            .map_err(|err| err.to_string())?;
        let ids = stmt
            .query_map(params![id], |r| r.get::<_, String>(0))
            .map_err(|err| err.to_string())?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|err| err.to_string())?;
        ids
    };

    if !owners.is_empty() {
        return Err("this profile still has chats — move them first".into());
    }

    conn.execute("DELETE FROM agent_profiles WHERE id = ?1", params![id])
        .map_err(|err| err.to_string())?;

    Ok(())
}

pub fn set_for_session(
    conn: &Connection,
    session_id: &str,
    profile_id: &str,
) -> Result<(), String> {
    get(conn, profile_id)?;

    let n = conn
        .execute(
            "UPDATE sessions SET profile_id = ?2 WHERE id = ?1",
            params![session_id, profile_id],
        )
        .map_err(|err| err.to_string())?;

    if n == 0 {
        return Err("session not found".into());
    }

    Ok(())
}

pub fn of_session(conn: &Connection, session_id: &str) -> Result<Option<String>, String> {
    conn.query_row(
        "SELECT profile_id FROM sessions WHERE id = ?1",
        params![session_id],
        |r| r.get::<_, Option<String>>(0),
    )
    .optional()
    .map(Option::flatten)
    .map_err(|err| err.to_string())
}
