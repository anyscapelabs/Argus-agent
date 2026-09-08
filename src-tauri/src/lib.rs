mod chat;
mod gateway;

use std::sync::Mutex;

use tauri::Manager;

use gateway::{store, Gateway};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
  tauri::Builder::default()
    .plugin(tauri_plugin_opener::init())
    .setup(|app| {
      let dir = app.path().app_data_dir()?;
      std::fs::create_dir_all(&dir)?;
      let conn = store::open(&dir.join("argus.db"))?;
      chat::store::migrate(&conn)?;
      let http = reqwest::Client::builder().build()?;
      app.manage(Gateway { conn: Mutex::new(conn), http });
      Ok(())
    })
    .invoke_handler(tauri::generate_handler![
      gateway::gw_list_providers,
      gateway::gw_upsert_provider,
      gateway::gw_list_models,
      gateway::gw_add_model,
      gateway::gw_link_model,
      gateway::gw_set_key,
      gateway::gw_set_routing,
      gateway::gw_chat,
      gateway::gw_chat_stream,
      gateway::gw_sync_catalog,
      chat::chat_create_session,
      chat::chat_list_sessions,
      chat::chat_save_session,
      chat::chat_delete_session,
      chat::chat_list_messages,
      chat::chat_add_message,
      chat::chat_supersede_from,
      chat::chat_create_folder,
      chat::chat_list_folders,
      gateway::gw_logs
    ])
    .run(tauri::generate_context!())
    .expect("error while running tauri application");
}
