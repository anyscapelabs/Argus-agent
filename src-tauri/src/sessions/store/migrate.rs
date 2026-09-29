// The one schema for every table this crate owns. Applied once at boot;
// every later read assumes it ran.
use rusqlite::{params, Connection};

pub fn migrate(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(crate::sessions::schema::MIGRATE)
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

    // The table is named per entry, not assumed: a column check against the
    // wrong table always passes and the ALTER then fails on boot.
    for (table, col, ddl) in [
        (
            "sessions",
            "parent_id",
            "ALTER TABLE sessions ADD COLUMN parent_id TEXT",
        ),
        (
            "sessions",
            "agent_name",
            "ALTER TABLE sessions ADD COLUMN agent_name TEXT",
        ),
        (
            "sessions",
            "agent_state",
            "ALTER TABLE sessions ADD COLUMN agent_state TEXT",
        ),
        (
            "sessions",
            "profile_id",
            "ALTER TABLE sessions ADD COLUMN profile_id TEXT",
        ),
        (
            "messages",
            "attachments",
            "ALTER TABLE messages ADD COLUMN attachments TEXT",
        ),
        (
            "messages",
            "local",
            "ALTER TABLE messages ADD COLUMN local INTEGER NOT NULL DEFAULT 0",
        ),
    ] {
        let has: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info(?1) WHERE name = ?2",
                params![table, col],
                |r| r.get::<_, i64>(0),
            )
            .map(|n| n > 0)
            .map_err(|err| err.to_string())?;

        if !has {
            conn.execute(ddl, []).map_err(|err| err.to_string())?;
        }
    }

    // The default is a real row rather than "a null profile means default",
    // so the picker has one list and never branches on a null. Chats that
    // predate profiles are pointed at it, so nothing existing has to migrate.
    conn.execute(
        "INSERT OR IGNORE INTO agent_profiles (id, name) VALUES ('default', '')",
        [],
    )
    .map_err(|err| err.to_string())?;

    conn.execute(
        "UPDATE sessions SET profile_id = 'default' WHERE profile_id IS NULL",
        [],
    )
    .map_err(|err| err.to_string())?;

    let has_reach: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('agent_profiles') WHERE name = 'reach_all'",
            [],
            |r| r.get::<_, i64>(0),
        )
        .map(|n| n > 0)
        .map_err(|err| err.to_string())?;

    if !has_reach {
        conn.execute(
            "ALTER TABLE agent_profiles ADD COLUMN reach_all INTEGER NOT NULL DEFAULT 0",
            [],
        )
        .map_err(|err| err.to_string())?;
    }

    conn.pragma_update(None, "foreign_keys", true)
        .map_err(|err| err.to_string())?;

    Ok(())
}
