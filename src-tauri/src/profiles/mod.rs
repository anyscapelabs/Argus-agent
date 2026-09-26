pub mod schema;
pub mod store;

use tauri::State;

use crate::gateway::Gateway;

#[tauri::command]
pub fn profile_list(gw: State<'_, Gateway>) -> Result<Vec<schema::Profile>, String> {
    let conn = gw.conn.lock().map_err(|err| err.to_string())?;
    store::list(&conn)
}

#[tauri::command]
pub fn profile_create(gw: State<'_, Gateway>, name: String) -> Result<schema::Profile, String> {
    let conn = gw.conn.lock().map_err(|err| err.to_string())?;
    store::create(&conn, &name)
}

#[tauri::command]
pub fn profile_edit(
    gw: State<'_, Gateway>,
    id: String,
    name: Option<String>,
    instructions: Option<String>,
) -> Result<schema::Profile, String> {
    let conn = gw.conn.lock().map_err(|err| err.to_string())?;
    store::edit(&conn, &id, name.as_deref(), instructions.as_deref())
}

#[tauri::command]
pub fn profile_delete(gw: State<'_, Gateway>, id: String) -> Result<(), String> {
    let conn = gw.conn.lock().map_err(|err| err.to_string())?;
    store::delete(&conn, &id)
}

#[tauri::command]
pub fn sess_set_profile(
    gw: State<'_, Gateway>,
    session_id: String,
    profile_id: String,
) -> Result<(), String> {
    let conn = gw.conn.lock().map_err(|err| err.to_string())?;
    store::set_for_session(&conn, &session_id, &profile_id)
}

#[tauri::command]
pub fn sess_get_profile(
    gw: State<'_, Gateway>,
    session_id: String,
) -> Result<Option<String>, String> {
    let conn = gw.conn.lock().map_err(|err| err.to_string())?;
    store::of_session(&conn, &session_id)
}
