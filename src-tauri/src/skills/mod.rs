pub mod schema;
pub mod store;

use std::path::PathBuf;

use tauri::State;

use crate::gateway::Gateway;
use schema::{NewSkill, Skill, UpdSkill};

#[tauri::command]
pub fn skill_create(gw: State<'_, Gateway>, skill: NewSkill) -> Result<Skill, String> {
    let conn = gw.conn.lock().map_err(|err| err.to_string())?;
    store::create_skill(&conn, &gw.skills_dir, &skill)
}

#[tauri::command]
pub fn skill_get(gw: State<'_, Gateway>, name: String) -> Result<Skill, String> {
    let conn = gw.conn.lock().map_err(|err| err.to_string())?;
    store::get_skill(&conn, &gw.skills_dir, &name)
}

#[tauri::command]
pub fn skill_list(gw: State<'_, Gateway>) -> Result<Vec<Skill>, String> {
    let conn = gw.conn.lock().map_err(|err| err.to_string())?;
    store::list_skills(&conn)
}

#[tauri::command]
pub fn skill_update(gw: State<'_, Gateway>, name: String, upd: UpdSkill) -> Result<Skill, String> {
    let conn = gw.conn.lock().map_err(|err| err.to_string())?;
    store::update_skill(&conn, &gw.skills_dir, &name, &upd)
}

#[tauri::command]
pub fn skill_delete(gw: State<'_, Gateway>, name: String) -> Result<(), String> {
    let conn = gw.conn.lock().map_err(|err| err.to_string())?;
    store::delete_skill(&conn, &gw.skills_dir, &name)
}

#[tauri::command]
pub fn skill_touch(gw: State<'_, Gateway>, name: String) -> Result<(), String> {
    let conn = gw.conn.lock().map_err(|err| err.to_string())?;
    store::touch_skill(&conn, &name)
}

#[tauri::command]
pub fn skill_search(
    gw: State<'_, Gateway>,
    query: String,
    limit: Option<i64>,
) -> Result<Vec<Skill>, String> {
    let conn = gw.conn.lock().map_err(|err| err.to_string())?;
    store::search_skills(&conn, &query, limit.unwrap_or(5))
}

#[tauri::command]
pub fn skill_sync(gw: State<'_, Gateway>) -> Result<usize, String> {
    let conn = gw.conn.lock().map_err(|err| err.to_string())?;
    store::sync(&conn, &gw.skills_dir)
}

pub fn default_dir(app_data: &PathBuf) -> PathBuf {
    app_data.join("skills")
}
