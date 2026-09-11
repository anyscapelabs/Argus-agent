mod gateway;
mod library;
mod prompt;
pub mod sessions;
mod skills;
pub mod tools;

use std::collections::HashMap;
use std::sync::Mutex;

use tauri::Manager;

use gateway::{store, Gateway};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    if std::env::args().any(|a| a == "--native-host") {
        let socket = sessions::ext_install::data_dir().join("native.sock");
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("tokio runtime"); // Loud fail: no runtime, no bridge

        rt.block_on(tools::extpipe::run_stdio_host(socket));
        return;
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&dir)?;

            let conn = store::open(&dir.join("argus.db"))?;
            sessions::store::migrate(&conn)?;
            skills::store::migrate(&conn)?;
            library::store::migrate(&conn)?;

            let skills_dir = skills::default_dir(&dir);
            std::fs::create_dir_all(&skills_dir)?;
            skills::store::sync(&conn, &skills_dir)?;

            let library_dir = library::default_dir(&dir);
            std::fs::create_dir_all(&library_dir)?;
            library::store::sync(&conn, &library_dir)?;

            let logos_dir = dir.join("logos");
            std::fs::create_dir_all(&logos_dir)?;

            tools::browser::init(dir.join("browser-profiles"));
            tools::extpipe::start_listener(dir.clone());

            if dir.join("extension.enabled").is_file() {
                let _ = sessions::ext_install::install_core(&dir);
            }

            let http = reqwest::Client::builder().build()?;

            app.manage(Gateway {
                conn: Mutex::new(conn),
                http,
                skills_dir,
                library_dir,
                logos_dir,
                approvals: Mutex::new(HashMap::new()),
            });

            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let gw = handle.state::<Gateway>();
                gateway::maybe_sync_catalog(gw.inner()).await;
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            gateway::gw_list_providers,
            gateway::gw_upsert_provider,
            gateway::gw_list_models,
            gateway::gw_provider_models,
            gateway::gw_chat_models,
            gateway::gw_set_model_enabled,
            gateway::gw_add_model,
            gateway::gw_link_model,
            gateway::gw_connect,
            gateway::gw_disconnect,
            gateway::gw_set_routing,
            gateway::gw_chat,
            gateway::gw_chat_stream,
            gateway::gw_sync_providers,
            gateway::gw_logo,
            sessions::sess_create_session,
            sessions::sess_list_sessions,
            sessions::sess_save_session,
            sessions::sess_set_permission,
            sessions::sess_set_model,
            sessions::sess_set_web_search,
            sessions::sess_delete_session,
            sessions::sess_export_json,
            sessions::sess_list_messages,
            sessions::sess_set_vote,
            sessions::sess_clean_dangling,
            sessions::sess_add_message,
            sessions::sess_supersede_from,
            sessions::sess_create_folder,
            sessions::sess_list_folders,
            sessions::chat::sess_chat_stream,
            sessions::chat::sess_resolve_approval,
            sessions::browser_import::sess_browser_import,
            sessions::ext_install::sess_ext_install,
            sessions::ext_install::sess_ext_status,
            sessions::ext_install::sess_ext_uninstall,
            skills::skill_create,
            skills::skill_get,
            skills::skill_list,
            skills::skill_update,
            skills::skill_delete,
            skills::skill_touch,
            skills::skill_search,
            skills::skill_sync,
            library::library_add,
            library::library_list,
            library::library_get,
            library::library_delete,
            library::library_search,
            library::library_path,
            gateway::gw_logs,
            prompt::prompt_preview,
            prompt::compressor::prompt_status,
            prompt::compressor::prompt_compact
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
