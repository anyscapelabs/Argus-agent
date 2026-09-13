use serde::Deserialize;

#[derive(Deserialize, Clone)]
pub struct OutlookCfg {
    pub client_id: String,
}

pub const SCOPES: &str = "offline_access User.Read Mail.ReadWrite Calendars.ReadWrite";

const ENV_ID: &str = "ARGUS_OUTLOOK_CLIENT_ID";
const APP_FILE: &str = "outlook-oauth.json";

pub fn load() -> Result<OutlookCfg, String> {
    if let Ok(Some(id)) = crate::mcp::vault::get_client("outlook") {
        if !id.trim().is_empty() {
            return Ok(OutlookCfg { client_id: id });
        }
    }

    if let Ok(id) = std::env::var(ENV_ID) {
        if !id.trim().is_empty() {
            return Ok(OutlookCfg { client_id: id });
        }
    }

    let app_path = crate::sessions::ext_install::data_dir().join(APP_FILE);
    if let Ok(raw) = std::fs::read_to_string(&app_path) {
        if let Ok(cfg) = serde_json::from_str::<OutlookCfg>(&raw) {
            if !cfg.client_id.trim().is_empty() {
                return Ok(cfg);
            }
        }
    }

    let dev_path =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/mcp/outlook/config.json");
    if let Ok(raw) = std::fs::read_to_string(&dev_path) {
        if let Ok(cfg) = serde_json::from_str::<OutlookCfg>(&raw) {
            if !cfg.client_id.trim().is_empty() {
                return Ok(cfg);
            }
        }
    }

    Err(format!(
        "outlook not configured — register an app at entra.microsoft.com and save its client id in Connectors setup, or set {ENV_ID}, or copy config.example.json to {} (app data)",
        app_path.display()
    ))
}
