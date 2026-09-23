pub mod browser_import;
pub mod chat;
pub mod ext_install;
pub mod schema;
pub mod store;

use tauri::State;

use crate::gateway::Gateway;
use schema::{Folder, Msg, NewMsg, NewSession, Session};

#[tauri::command]
pub fn sess_create_session(gw: State<'_, Gateway>, req: NewSession) -> Result<Session, String> {
    let conn = gw.conn.lock().map_err(|err| err.to_string())?;
    store::create_session(&conn, &req)
}

#[tauri::command]
pub fn sess_list_sessions(
    gw: State<'_, Gateway>,
    folder_id: Option<String>,
) -> Result<Vec<Session>, String> {
    let conn = gw.conn.lock().map_err(|err| err.to_string())?;
    store::list_sessions(&conn, folder_id.as_deref())
}

#[tauri::command]
pub fn sess_save_session(gw: State<'_, Gateway>, session: Session) -> Result<(), String> {
    let conn = gw.conn.lock().map_err(|err| err.to_string())?;
    store::save_session(&conn, &session)
}

#[tauri::command]
pub fn sess_set_permission(
    gw: State<'_, Gateway>,
    session_id: String,
    permission: String,
) -> Result<(), String> {
    let conn = gw.conn.lock().map_err(|err| err.to_string())?;
    store::set_permission(&conn, &session_id, &permission)
}

#[tauri::command]
pub fn sess_set_reflect(
    gw: State<'_, Gateway>,
    session_id: String,
    on: bool,
) -> Result<(), String> {
    let conn = gw.conn.lock().map_err(|err| err.to_string())?;
    store::set_reflect(&conn, &session_id, on)
}

#[tauri::command]
pub fn sess_set_model(
    gw: State<'_, Gateway>,
    session_id: String,
    model_id: Option<String>,
) -> Result<(), String> {
    let conn = gw.conn.lock().map_err(|err| err.to_string())?;
    store::set_model(&conn, &session_id, model_id.as_deref())
}

#[tauri::command]
pub fn sess_set_web_search(
    gw: State<'_, Gateway>,
    session_id: String,
    on: bool,
) -> Result<(), String> {
    let conn = gw.conn.lock().map_err(|err| err.to_string())?;
    store::set_web_search(&conn, &session_id, on)
}

#[tauri::command]
pub fn sess_delete_session(gw: State<'_, Gateway>, session_id: String) -> Result<(), String> {
    let conn = gw.conn.lock().map_err(|err| err.to_string())?;
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

    let conn = gw.conn.lock().map_err(|err| err.to_string())?;
    store::set_vote(&conn, &session_id, &msg_id, v.as_deref())?;
    crate::learning::record_vote(&conn, &session_id, &msg_id, v.as_deref())?;

    Ok(())
}

#[tauri::command]
pub fn sess_export_json(gw: State<'_, Gateway>, session_id: String) -> Result<String, String> {
    let conn = gw.conn.lock().map_err(|err| err.to_string())?;
    store::export_json(&conn, &session_id)
}

#[tauri::command]
pub fn sess_list_messages(gw: State<'_, Gateway>, session_id: String) -> Result<Vec<Msg>, String> {
    let conn = gw.conn.lock().map_err(|err| err.to_string())?;
    store::list_msgs(&conn, &session_id)
}

#[tauri::command]
pub fn sess_add_message(gw: State<'_, Gateway>, msg: NewMsg) -> Result<Msg, String> {
    let conn = gw.conn.lock().map_err(|err| err.to_string())?;
    store::add_msg(&conn, &msg)
}

#[tauri::command]
pub fn sess_supersede_from(
    gw: State<'_, Gateway>,
    session_id: String,
    seq: i64,
) -> Result<(), String> {
    let conn = gw.conn.lock().map_err(|err| err.to_string())?;
    store::supersede_from(&conn, &session_id, seq)
}

#[tauri::command]
pub fn sess_clean_dangling(gw: State<'_, Gateway>, session_id: String) -> Result<usize, String> {
    let conn = gw.conn.lock().map_err(|err| err.to_string())?;
    store::clean_dangling(&conn, &session_id)
}

#[tauri::command]
pub fn sess_create_folder(gw: State<'_, Gateway>, name: String) -> Result<Folder, String> {
    let conn = gw.conn.lock().map_err(|err| err.to_string())?;
    store::create_folder(&conn, &name)
}

#[tauri::command]
pub fn sess_list_folders(gw: State<'_, Gateway>) -> Result<Vec<Folder>, String> {
    let conn = gw.conn.lock().map_err(|err| err.to_string())?;
    store::list_folders(&conn)
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NewAgentPrefs {
    pub model_id: Option<String>,
    pub permission: String,
    pub web_search: bool,
}

#[tauri::command]
pub fn newagent_prefs(gw: State<'_, Gateway>) -> Result<NewAgentPrefs, String> {
    use crate::gateway::store as gw_store;

    let conn = gw.conn.lock().map_err(|err| err.to_string())?;

    Ok(NewAgentPrefs {
        model_id: gw_store::kv_get(&conn, "newagent.model").filter(|m| !m.is_empty()),
        permission: gw_store::kv_get(&conn, "newagent.permission").unwrap_or_else(|| "ask".into()),
        web_search: gw_store::kv_get(&conn, "newagent.websearch").as_deref() == Some("1"),
    })
}

#[tauri::command]
pub fn set_newagent_prefs(
    gw: State<'_, Gateway>,
    model_id: Option<String>,
    permission: String,
    web_search: bool,
) -> Result<(), String> {
    use crate::gateway::store as gw_store;

    let conn = gw.conn.lock().map_err(|err| err.to_string())?;

    gw_store::kv_set(&conn, "newagent.model", model_id.as_deref().unwrap_or(""))?;
    gw_store::kv_set(&conn, "newagent.permission", &permission)?;
    gw_store::kv_set(
        &conn,
        "newagent.websearch",
        if web_search { "1" } else { "0" },
    )?;

    Ok(())
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TermShellStatus {
    pub binary: String,
    pub kind: String,
    pub version: Option<String>,
    pub source: String,
}

fn shell_status() -> TermShellStatus {
    let cfg = crate::tools::shell::detect::status();

    let kind = match cfg.kind {
        crate::tools::shell::ShellKind::Bash => "bash",
        crate::tools::shell::ShellKind::Sh => "sh",
        crate::tools::shell::ShellKind::Wsl => "wsl",
        crate::tools::shell::ShellKind::PowerShell => "powershell",
        crate::tools::shell::ShellKind::Cmd => "cmd",
    };

    TermShellStatus {
        binary: cfg.binary.to_string_lossy().into_owned(),
        kind: kind.into(),
        version: cfg.version,
        source: if crate::tools::shell::detect::override_active() {
            "override".into()
        } else {
            "auto".into()
        },
    }
}

#[tauri::command]
pub fn term_shell_status() -> TermShellStatus {
    shell_status()
}

#[tauri::command]
pub fn term_shell_set(gw: State<'_, Gateway>, path: String) -> Result<TermShellStatus, String> {
    use crate::gateway::store as gw_store;

    let cfg = crate::tools::shell::detect::set_override(&path)?;

    let conn = gw.conn.lock().map_err(|err| err.to_string())?;
    gw_store::kv_set(&conn, "terminal.shell", &cfg.binary.to_string_lossy())?;

    Ok(shell_status())
}

#[tauri::command]
pub fn term_shell_clear(gw: State<'_, Gateway>) -> Result<TermShellStatus, String> {
    use crate::gateway::store as gw_store;

    crate::tools::shell::detect::clear_override();

    let conn = gw.conn.lock().map_err(|err| err.to_string())?;
    gw_store::kv_set(&conn, "terminal.shell", "")?;

    Ok(shell_status())
}
