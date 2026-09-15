pub mod schema;
pub mod store;

use tauri::State;

use crate::gateway::Gateway;
use schema::{Memory, MemoryGraph, MemoryLink, NewMemory, RecallHit};

#[tauri::command]
pub fn memory_save(gw: State<'_, Gateway>, mem: NewMemory) -> Result<Memory, String> {
    let conn = gw.conn.lock().map_err(|err| err.to_string())?;
    store::save(&conn, &mem)
}

#[tauri::command]
pub fn memory_get(gw: State<'_, Gateway>, id: String) -> Result<Memory, String> {
    let conn = gw.conn.lock().map_err(|err| err.to_string())?;
    store::get(&conn, &id)
}

#[tauri::command]
pub fn memory_list(
    gw: State<'_, Gateway>,
    kind: Option<String>,
    limit: Option<i64>,
) -> Result<Vec<Memory>, String> {
    let conn = gw.conn.lock().map_err(|err| err.to_string())?;
    store::list(&conn, kind.as_deref(), limit.unwrap_or(50))
}

#[tauri::command]
pub fn memory_delete(gw: State<'_, Gateway>, id: String) -> Result<(), String> {
    let conn = gw.conn.lock().map_err(|err| err.to_string())?;
    store::delete(&conn, &id)
}

#[tauri::command]
pub fn memory_search(
    gw: State<'_, Gateway>,
    query: String,
    limit: Option<i64>,
) -> Result<Vec<Memory>, String> {
    let conn = gw.conn.lock().map_err(|err| err.to_string())?;
    store::search(&conn, &query, limit.unwrap_or(10))
}

#[tauri::command]
pub fn memory_link(
    gw: State<'_, Gateway>,
    from_id: String,
    to_id: String,
    relation: Option<String>,
) -> Result<MemoryLink, String> {
    let conn = gw.conn.lock().map_err(|err| err.to_string())?;
    store::link(&conn, &from_id, &to_id, &relation.unwrap_or_else(|| "related".into()))
}

#[tauri::command]
pub fn memory_unlink(gw: State<'_, Gateway>, from_id: String, to_id: String) -> Result<(), String> {
    let conn = gw.conn.lock().map_err(|err| err.to_string())?;
    store::unlink(&conn, &from_id, &to_id)
}

#[tauri::command]
pub fn memory_recall(
    gw: State<'_, Gateway>,
    query: String,
    limit: Option<i64>,
) -> Result<Vec<RecallHit>, String> {
    let conn = gw.conn.lock().map_err(|err| err.to_string())?;
    store::recall(&conn, &query, limit.unwrap_or(12))
}

#[tauri::command]
pub fn memory_graph(gw: State<'_, Gateway>, limit: Option<i64>) -> Result<MemoryGraph, String> {
    let conn = gw.conn.lock().map_err(|err| err.to_string())?;
    store::load_graph(&conn, limit.unwrap_or(80))
}
