pub mod chat;
pub mod schema;
pub mod store;

use tauri::State;

use crate::gateway::Gateway;
use schema::{Folder, Msg, NewMsg, NewSession, Session};

#[tauri::command]
pub fn sess_create_session(gw: State<'_, Gateway>, req: NewSession) -> Result<Session, String> {
  let conn = gw.conn.lock().map_err(|e| e.to_string())?;
  store::create_session(&conn, &req)
}

#[tauri::command]
pub fn sess_list_sessions(
  gw: State<'_, Gateway>,
  folder_id: Option<String>,
) -> Result<Vec<Session>, String> {
  let conn = gw.conn.lock().map_err(|e| e.to_string())?;
  store::list_sessions(&conn, folder_id.as_deref())
}

#[tauri::command]
pub fn sess_save_session(gw: State<'_, Gateway>, session: Session) -> Result<(), String> {
  let conn = gw.conn.lock().map_err(|e| e.to_string())?;
  store::save_session(&conn, &session)
}

#[tauri::command]
pub fn sess_set_permission(
  gw: State<'_, Gateway>,
  session_id: String,
  permission: String,
) -> Result<(), String> {
  let conn = gw.conn.lock().map_err(|e| e.to_string())?;
  store::set_permission(&conn, &session_id, &permission)
}

#[tauri::command]
pub fn sess_set_model(
  gw: State<'_, Gateway>,
  session_id: String,
  model_id: Option<String>,
) -> Result<(), String> {
  let conn = gw.conn.lock().map_err(|e| e.to_string())?;
  store::set_model(&conn, &session_id, model_id.as_deref())
}

#[tauri::command]
pub fn sess_delete_session(gw: State<'_, Gateway>, session_id: String) -> Result<(), String> {
  let conn = gw.conn.lock().map_err(|e| e.to_string())?;
  store::delete_session(&conn, &session_id)
}

#[tauri::command]
pub fn sess_set_vote(
  gw: State<'_, Gateway>,
  session_id: String,
  msg_id: String,
  vote: Option<String>,
) -> Result<(), String> {
  let v = match vote.as_deref() {
    Some("up") | Some("down") => vote,
    _ => None,
  };
  let conn = gw.conn.lock().map_err(|e| e.to_string())?;
  store::set_vote(&conn, &session_id, &msg_id, v.as_deref())
}

#[tauri::command]
pub fn sess_export_json(gw: State<'_, Gateway>, session_id: String) -> Result<String, String> {
  let conn = gw.conn.lock().map_err(|e| e.to_string())?;
  store::export_json(&conn, &session_id)
}

#[tauri::command]
pub fn sess_list_messages(gw: State<'_, Gateway>, session_id: String) -> Result<Vec<Msg>, String> {
  let conn = gw.conn.lock().map_err(|e| e.to_string())?;
  store::list_msgs(&conn, &session_id)
}

#[tauri::command]
pub fn sess_add_message(gw: State<'_, Gateway>, msg: NewMsg) -> Result<Msg, String> {
  let conn = gw.conn.lock().map_err(|e| e.to_string())?;
  store::add_msg(&conn, &msg)
}

#[tauri::command]
pub fn sess_supersede_from(
  gw: State<'_, Gateway>,
  session_id: String,
  seq: i64,
) -> Result<(), String> {
  let conn = gw.conn.lock().map_err(|e| e.to_string())?;
  store::supersede_from(&conn, &session_id, seq)
}

#[tauri::command]
pub fn sess_clean_dangling(gw: State<'_, Gateway>, session_id: String) -> Result<usize, String> {
  let conn = gw.conn.lock().map_err(|e| e.to_string())?;
  store::clean_dangling(&conn, &session_id)
}

#[tauri::command]
pub fn sess_create_folder(gw: State<'_, Gateway>, name: String) -> Result<Folder, String> {
  let conn = gw.conn.lock().map_err(|e| e.to_string())?;
  store::create_folder(&conn, &name)
}

#[tauri::command]
pub fn sess_list_folders(gw: State<'_, Gateway>) -> Result<Vec<Folder>, String> {
  let conn = gw.conn.lock().map_err(|e| e.to_string())?;
  store::list_folders(&conn)
}
