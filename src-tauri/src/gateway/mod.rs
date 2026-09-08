pub mod adapters;
pub mod catalog;
pub mod router;
pub mod schema;
pub mod store;

use std::sync::Mutex;

use reqwest::Client;
use rusqlite::Connection;
use tauri::State;

use schema::{Avail, ChatReq, ChatResp, ModelEntry, Provider};

pub struct Gateway {
  pub conn: Mutex<Connection>,
  pub http: Client,
}

#[tauri::command]
pub fn gw_list_providers(gw: State<'_, Gateway>) -> Result<Vec<Provider>, String> {
  let conn = gw.conn.lock().map_err(|e| e.to_string())?;
  store::list_providers(&conn)
}

#[tauri::command]
pub fn gw_upsert_provider(gw: State<'_, Gateway>, prov: Provider) -> Result<(), String> {
  let conn = gw.conn.lock().map_err(|e| e.to_string())?;
  store::upsert_provider(&conn, &prov)
}

#[tauri::command]
pub fn gw_list_models(gw: State<'_, Gateway>) -> Result<Vec<ModelEntry>, String> {
  let conn = gw.conn.lock().map_err(|e| e.to_string())?;
  store::list_models(&conn)
}

#[tauri::command]
pub fn gw_add_model(gw: State<'_, Gateway>, model: ModelEntry) -> Result<(), String> {
  let conn = gw.conn.lock().map_err(|e| e.to_string())?;
  store::add_model(&conn, &model)
}

#[tauri::command]
pub fn gw_link_model(gw: State<'_, Gateway>, avail: Avail) -> Result<(), String> {
  let conn = gw.conn.lock().map_err(|e| e.to_string())?;
  store::link_model(&conn, &avail)
}

#[tauri::command]
pub fn gw_set_key(provider_id: String, tok: String) -> Result<(), String> {
  if tok.is_empty() {
    return store::secret_del(&provider_id); // Drop it
  }
  store::secret_set(&provider_id, &tok)
}

#[tauri::command]
pub fn gw_set_routing(gw: State<'_, Gateway>, mode: String, pinned: String) -> Result<(), String> {
  let conn = gw.conn.lock().map_err(|e| e.to_string())?;
  store::kv_set(&conn, "routing_mode", &mode)?;
  store::kv_set(&conn, "pinned_provider", &pinned)
}

#[tauri::command]
pub async fn gw_chat(gw: State<'_, Gateway>, req: ChatReq) -> Result<ChatResp, String> {
  router::run(&gw, &req).await
}

#[tauri::command]
pub fn gw_logs(gw: State<'_, Gateway>, limit: Option<i64>) -> Result<Vec<schema::ReqLog>, String> {
  let conn = gw.conn.lock().map_err(|e| e.to_string())?;
  store::list_logs(&conn, limit.unwrap_or(100))
}
