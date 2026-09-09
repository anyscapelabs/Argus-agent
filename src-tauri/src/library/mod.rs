pub mod schema;
pub mod store;

use std::path::PathBuf;

use tauri::State;

use crate::gateway::Gateway;
use schema::{LibItem, NewLibItem};

#[tauri::command]
pub fn library_add(gw: State<'_, Gateway>, item: NewLibItem) -> Result<LibItem, String> {
    let conn = gw.conn.lock().map_err(|e| e.to_string())?;
    store::add(&conn, &gw.library_dir, &item)
}

#[tauri::command]
pub fn library_list(gw: State<'_, Gateway>, kind: Option<String>) -> Result<Vec<LibItem>, String> {
    let conn = gw.conn.lock().map_err(|e| e.to_string())?;
    store::list(&conn, kind.as_deref())
}

#[tauri::command]
pub fn library_get(gw: State<'_, Gateway>, id: String) -> Result<LibItem, String> {
    let conn = gw.conn.lock().map_err(|e| e.to_string())?;
    store::get(&conn, &gw.library_dir, &id)
}

#[tauri::command]
pub fn library_delete(gw: State<'_, Gateway>, id: String) -> Result<(), String> {
    let conn = gw.conn.lock().map_err(|e| e.to_string())?;
    store::delete(&conn, &gw.library_dir, &id)
}

#[tauri::command]
pub fn library_search(
    gw: State<'_, Gateway>,
    query: String,
    limit: Option<i64>,
) -> Result<Vec<LibItem>, String> {
    let conn = gw.conn.lock().map_err(|e| e.to_string())?;
    store::search(&conn, &query, limit.unwrap_or(10))
}

#[tauri::command]
pub fn library_path(gw: State<'_, Gateway>, id: String) -> Result<String, String> {
    let conn = gw.conn.lock().map_err(|e| e.to_string())?;
    let item = store::get(&conn, &gw.library_dir, &id)?;
    Ok(gw
        .library_dir
        .join(&item.path)
        .to_string_lossy()
        .into_owned())
}

pub fn default_dir(app_data: &PathBuf) -> PathBuf {
    app_data.join("library")
}
