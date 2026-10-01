use rusqlite::{params, Connection, OptionalExtension};
use uuid::Uuid;

use crate::gateway::store as gw_store;

use super::schema::{Grant, Profile, Reach, CAPABILITIES, DEFAULT_ID, MAX_PROFILES, NAME_MAX};

fn row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Profile> {
    Ok(Profile {
        id: r.get(0)?,
        name: r.get(1)?,
        instructions: r.get::<_, Option<String>>(2)?.unwrap_or_default(),
        reach_all: r.get::<_, i64>(3)? != 0,
        grants: r.get(4)?,
        created_at: r.get(5)?,
    })
}

const COLS: &str = "id, name, instructions, reach_all, \
                    (SELECT COUNT(*) FROM profile_grants g \
                       WHERE g.profile_id = agent_profiles.id), created_at";

/// The default first, then newest: the default reads as "Default" in the UI, so
/// it belongs at the top.
pub fn list(conn: &Connection) -> Result<Vec<Profile>, String> {
    let mut stmt = conn
        .prepare(&format!(
            "SELECT {COLS} FROM agent_profiles \
             ORDER BY (id = 'default') DESC, created_at DESC, rowid DESC"
        ))
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
        &format!("SELECT {COLS} FROM agent_profiles WHERE id = ?1"),
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
    // Loud on a missing profile, not a silent no-op that reads as "saved".
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

    // Refuse while it still owns work, so the user is not left wondering what
    // happened to the chat they just opened.
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

/// Which profile the picker was left on. Survives a restart: finding a
/// different personality than the one you closed the app with is its own
/// annoyance every single time.
pub fn active(conn: &Connection) -> String {
    gw_store::kv_get(conn, "profile.active")
        .filter(|id| !id.is_empty())
        .unwrap_or_else(|| DEFAULT_ID.into())
}

pub fn set_active(conn: &Connection, id: &str) -> Result<(), String> {
    get(conn, id)?;
    gw_store::kv_set(conn, "profile.active", id)
}

/// Read on demand, not shipped with `list()`: a full matrix is sixty cells and
/// the settings list does not draw them.
pub fn reach(conn: &Connection, id: &str) -> Result<Reach, String> {
    let p = get(conn, id)?;

    let mut stmt = conn
        .prepare("SELECT capability, target_id FROM profile_grants WHERE profile_id = ?1")
        .map_err(|err| err.to_string())?;

    let grants = stmt
        .query_map(params![id], |r| {
            Ok(Grant {
                capability: r.get(0)?,
                target_id: r.get(1)?,
            })
        })
        .map_err(|err| err.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|err| err.to_string())?;

    Ok(Reach {
        reach_all: p.reach_all,
        grants,
    })
}

pub fn set_reach(
    conn: &Connection,
    id: &str,
    reach_all: bool,
    grants: Vec<Grant>,
) -> Result<(), String> {
    get(conn, id)?;

    for g in &grants {
        if !CAPABILITIES.contains(&g.capability.as_str()) {
            return Err(format!("unknown capability: {}", g.capability));
        }

        // No second, unchecked copy of the same permissions.
        if g.target_id == id {
            return Err("a profile cannot be granted access to itself".into());
        }

        get(conn, &g.target_id)?;
    }

    conn.execute(
        "UPDATE agent_profiles SET reach_all = ?2 WHERE id = ?1",
        params![id, i64::from(reach_all)],
    )
    .map_err(|err| err.to_string())?;

    // Replaced wholesale: a half-sent matrix would be a half-revoked one.
    conn.execute(
        "DELETE FROM profile_grants WHERE profile_id = ?1",
        params![id],
    )
    .map_err(|err| err.to_string())?;

    for g in &grants {
        // A matrix is a set of cells: a double click is not a second grant.
        conn.execute(
            "INSERT OR IGNORE INTO profile_grants (profile_id, capability, target_id) \
             VALUES (?1, ?2, ?3)",
            params![id, g.capability, g.target_id],
        )
        .map_err(|err| err.to_string())?;
    }

    Ok(())
}
