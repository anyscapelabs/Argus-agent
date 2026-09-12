use rusqlite::{params, Connection, OptionalExtension};

use super::Connector;

pub const MIGRATE: &str = r#"
CREATE TABLE IF NOT EXISTS connectors (
  id         TEXT PRIMARY KEY,
  enabled    INTEGER NOT NULL DEFAULT 0,
  command    TEXT NOT NULL,
  args_json  TEXT NOT NULL DEFAULT '[]',
  env_json   TEXT NOT NULL DEFAULT '{}',
  status     TEXT NOT NULL DEFAULT 'off',
  last_error TEXT
);
"#;

pub fn migrate(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(MIGRATE).map_err(|err| err.to_string())
}

pub fn enabled(conn: &Connection) -> Result<Vec<Connector>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, command, args_json, env_json FROM connectors \
             WHERE enabled = 1 ORDER BY id",
        )
        .map_err(|err| err.to_string())?;

    let map = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
            ))
        })
        .map_err(|err| err.to_string())?;

    let rows = map
        .collect::<Result<Vec<_>, _>>()
        .map_err(|err| err.to_string())?;

    Ok(rows
        .into_iter()
        .filter_map(|(id, command, args_json, env_json)| {
            let args: serde_json::Value = serde_json::from_str(&args_json).ok()?;
            let env: serde_json::Value = serde_json::from_str(&env_json).ok()?;

            Some(Connector {
                id,
                command,
                args: args
                    .as_array()?
                    .iter()
                    .filter_map(|v| v.as_str().map(Into::into))
                    .collect(),
                env: env
                    .as_object()?
                    .iter()
                    .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                    .collect(),
            })
        })
        .collect())
}

pub fn put(
    conn: &Connection,
    id: &str,
    command: &str,
    args: &[String],
    env: &[(String, String)],
) -> Result<(), String> {
    let args_json = serde_json::to_string(args).map_err(|err| err.to_string())?;
    let env_map: serde_json::Map<String, serde_json::Value> = env
        .iter()
        .map(|(k, v)| (k.clone(), serde_json::Value::String(v.clone())))
        .collect();
    let env_json = serde_json::Value::Object(env_map).to_string();

    conn.execute(
        "INSERT INTO connectors (id, command, args_json, env_json) \
         VALUES (?1, ?2, ?3, ?4) \
         ON CONFLICT(id) DO UPDATE SET command=?2, args_json=?3, env_json=?4",
        params![id, command, args_json, env_json],
    )
    .map_err(|err| err.to_string())?;

    Ok(())
}

pub fn set_enabled(conn: &Connection, id: &str, on: bool) -> Result<(), String> {
    conn.execute(
        "UPDATE connectors SET enabled = ?2, status = 'off', last_error = NULL WHERE id = ?1",
        params![id, on as i64],
    )
    .map_err(|err| err.to_string())?;

    Ok(())
}

pub fn set_status(
    conn: &Connection,
    id: &str,
    status: &str,
    err: Option<&str>,
) -> Result<(), String> {
    conn.execute(
        "UPDATE connectors SET status = ?2, last_error = ?3 WHERE id = ?1",
        params![id, status, err],
    )
    .map_err(|err| err.to_string())?;

    Ok(())
}

pub fn get(conn: &Connection, id: &str) -> Result<Option<(bool, String, String)>, String> {
    conn.query_row(
        "SELECT enabled, status, last_error FROM connectors WHERE id = ?1",
        params![id],
        |r| {
            let err: Option<String> = r.get(2)?;
            Ok((r.get::<_, i64>(0)? != 0, r.get(1)?, err.unwrap_or_default()))
        },
    )
    .optional()
    .map_err(|err| err.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mem() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        conn
    }

    #[test]
    fn put_enable_and_read_back() {
        let c = mem();

        put(
            &c,
            "gmail",
            "npx",
            &["-y".to_string(), "@x/gmail-mcp".to_string()],
            &[("CLIENT_ID".into(), "abc".into())],
        )
        .unwrap();
        set_enabled(&c, "gmail", true).unwrap();

        let list = enabled(&c).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, "gmail");
        assert_eq!(list[0].args, vec!["-y", "@x/gmail-mcp"]);
        assert_eq!(list[0].env, vec![("CLIENT_ID".into(), "abc".into())]);

        let (on, status, _) = get(&c, "gmail").unwrap().unwrap();
        assert!(on);
        assert_eq!(status, "off");
    }

    #[test]
    fn disabled_connectors_are_not_listed() {
        let c = mem();

        put(
            &c,
            "cal",
            "npx",
            &["-y".to_string(), "@x/cal".to_string()],
            &[],
        )
        .unwrap();

        assert!(enabled(&c).unwrap().is_empty());

        set_enabled(&c, "cal", true).unwrap();
        set_enabled(&c, "cal", false).unwrap();

        assert!(enabled(&c).unwrap().is_empty());
    }

    #[test]
    fn put_upserts_launch_config() {
        let c = mem();

        put(
            &c,
            "gmail",
            "npx",
            &["-y".to_string(), "old".to_string()],
            &[],
        )
        .unwrap();
        put(
            &c,
            "gmail",
            "npx",
            &["-y".to_string(), "new".to_string()],
            &[],
        )
        .unwrap();
        set_enabled(&c, "gmail", true).unwrap();

        assert_eq!(enabled(&c).unwrap()[0].args, vec!["-y", "new"]);
    }
}
