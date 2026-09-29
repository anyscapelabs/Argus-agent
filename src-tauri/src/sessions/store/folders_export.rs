// Folders and the session export. Both are leaves — nothing here is called
// from the other store sections.
use rusqlite::{params, Connection};
use uuid::Uuid;

use super::sessions::get_session;
use crate::sessions::schema::Folder;

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
