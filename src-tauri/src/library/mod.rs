pub mod doc;
pub mod schema;
pub mod store;

use std::path::PathBuf;

use tauri::State;

use crate::gateway::Gateway;
use schema::{LibDownload, LibItem, LibPreview, NewLibItem};

#[tauri::command]
pub fn library_add(gw: State<'_, Gateway>, item: NewLibItem) -> Result<LibItem, String> {
    let conn = gw.conn.lock().map_err(|err| err.to_string())?;
    store::add(&conn, &gw.library_dir, &item)
}

#[tauri::command]
pub fn library_list(gw: State<'_, Gateway>, kind: Option<String>) -> Result<Vec<LibItem>, String> {
    let conn = gw.conn.lock().map_err(|err| err.to_string())?;
    store::list(&conn, kind.as_deref())
}

#[tauri::command]
pub fn library_get(gw: State<'_, Gateway>, id: String) -> Result<LibItem, String> {
    let conn = gw.conn.lock().map_err(|err| err.to_string())?;
    store::get(&conn, &gw.library_dir, &id)
}

#[tauri::command]
pub fn library_delete(gw: State<'_, Gateway>, id: String) -> Result<(), String> {
    let conn = gw.conn.lock().map_err(|err| err.to_string())?;
    store::delete(&conn, &gw.library_dir, &id)
}

#[tauri::command]
pub fn library_search(
    gw: State<'_, Gateway>,
    query: String,
    limit: Option<i64>,
) -> Result<Vec<LibItem>, String> {
    let conn = gw.conn.lock().map_err(|err| err.to_string())?;
    store::search(&conn, &query, limit.unwrap_or(10))
}

#[tauri::command]
pub fn library_path(gw: State<'_, Gateway>, id: String) -> Result<String, String> {
    let conn = gw.conn.lock().map_err(|err| err.to_string())?;
    let item = store::get(&conn, &gw.library_dir, &id)?;
    Ok(gw
        .library_dir
        .join(&item.path)
        .to_string_lossy()
        .into_owned())
}

#[tauri::command]
pub fn library_download(gw: State<'_, Gateway>, id: String) -> Result<LibDownload, String> {
    let conn = gw.conn.lock().map_err(|err| err.to_string())?;
    store::download(&conn, &gw.library_dir, &id)
}

#[tauri::command]
pub fn library_preview(
    gw: State<'_, Gateway>,
    id: String,
    max_chars: Option<i64>,
) -> Result<LibPreview, String> {
    let conn = gw.conn.lock().map_err(|err| err.to_string())?;
    let max = max_chars.unwrap_or(200000).clamp(1000, 200000) as usize;
    store::preview(&conn, &gw.library_dir, &id, max)
}

#[tauri::command]
pub fn library_create_doc(
    gw: State<'_, Gateway>,
    name: String,
    kind: String,
    title: Option<String>,
    content: Option<String>,
    rows: Option<serde_json::Value>,
    slides: Option<serde_json::Value>,
    session_id: Option<String>,
) -> Result<LibItem, String> {
    let args = serde_json::json!({
        "name": name,
        "kind": kind,
        "title": title.unwrap_or_default(),
        "content": content.unwrap_or_default(),
        "rows": rows.unwrap_or(serde_json::Value::Null),
        "slides": slides.unwrap_or(serde_json::Value::Null),
    });
    let (item, _) = doc::create(&gw, &args, session_id.as_deref())?;
    Ok(item)
}

pub fn default_dir(app_data: &PathBuf) -> PathBuf {
    app_data.join("library")
}
