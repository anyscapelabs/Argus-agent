use serde::Deserialize;

#[derive(Deserialize, Clone)]
pub struct GoogleCfg {
    pub client_id: String,
    pub client_secret: String,
}

pub const SCOPES: &[&str] = &[
    "openid",
    "email",
    "profile",
    "https://www.googleapis.com/auth/gmail.modify",
    "https://www.googleapis.com/auth/calendar",
    "https://www.googleapis.com/auth/drive",
    "https://www.googleapis.com/auth/documents",
    "https://www.googleapis.com/auth/spreadsheets",
];

const ENV_ID: &str = "ARGUS_GOOGLE_CLIENT_ID";
const ENV_SECRET: &str = "ARGUS_GOOGLE_CLIENT_SECRET";
const APP_FILE: &str = "google-oauth.json";

// Baked at compile time from the build environment, so release builds carry
// the secrets CI injects. A shipped app has no build environment to read.
const BAKED_ID: Option<&str> = option_env!("ARGUS_GOOGLE_CLIENT_ID");
const BAKED_SECRET: Option<&str> = option_env!("ARGUS_GOOGLE_CLIENT_SECRET");

pub fn load() -> Result<GoogleCfg, String> {
    if let Ok(Some(id)) = crate::mcp::vault::get_client("google") {
        if !id.trim().is_empty() {
            return Ok(GoogleCfg {
                client_id: id,
                client_secret: crate::mcp::vault::get_secret("google")
                    .ok()
                    .flatten()
                    .unwrap_or_default(),
            });
        }
    }

    if let (Ok(id), Ok(secret)) = (std::env::var(ENV_ID), std::env::var(ENV_SECRET)) {
        if !id.trim().is_empty() {
            return Ok(GoogleCfg {
                client_id: id,
                client_secret: secret,
            });
        }
    }

    if let (Some(id), Some(secret)) = (BAKED_ID, BAKED_SECRET) {
        if !id.trim().is_empty() {
            return Ok(GoogleCfg {
                client_id: id.into(),
                client_secret: secret.into(),
            });
        }
    }

    let app_path = crate::sessions::ext_install::data_dir().join(APP_FILE);
    if let Ok(raw) = std::fs::read_to_string(&app_path) {
        if let Ok(cfg) = serde_json::from_str::<GoogleCfg>(&raw) {
            if !cfg.client_id.trim().is_empty() {
                return Ok(cfg);
            }
        }
    }

    let dev_path =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/mcp/google/config.json");
    if let Ok(raw) = std::fs::read_to_string(&dev_path) {
        if let Ok(cfg) = serde_json::from_str::<GoogleCfg>(&raw) {
            if !cfg.client_id.trim().is_empty() {
                return Ok(cfg);
            }
        }
    }

    Err(format!(
        "google oauth not configured — save your own client id in Connectors setup, or set {ENV_ID}/{ENV_SECRET}, or copy config.example.json to {} (app data)",
        app_path.display()
    ))
}
