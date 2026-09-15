use serde::Deserialize;

#[derive(Deserialize, Clone)]
pub struct TrelloCfg {
    pub api_key: String,
}

const ENV_KEY: &str = "ARGUS_TRELLO_KEY";
const APP_FILE: &str = "trello-settings.json";

pub fn load() -> Result<TrelloCfg, String> {
    if let Ok(key) = std::env::var(ENV_KEY) {
        if !key.trim().is_empty() {
            return Ok(TrelloCfg { api_key: key });
        }
    }

    let app_path = crate::sessions::ext_install::data_dir().join(APP_FILE);
    if let Ok(raw) = std::fs::read_to_string(&app_path) {
        if let Ok(cfg) = serde_json::from_str::<TrelloCfg>(&raw) {
            if !cfg.api_key.trim().is_empty() {
                return Ok(cfg);
            }
        }
    }

    let dev_path =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/mcp/trello/config.json");
    if let Ok(raw) = std::fs::read_to_string(&dev_path) {
        if let Ok(cfg) = serde_json::from_str::<TrelloCfg>(&raw) {
            if !cfg.api_key.trim().is_empty() {
                return Ok(cfg);
            }
        }
    }

    Err(format!(
        "trello api key not configured — copy config.example.json to {} (app data) or set {ENV_KEY}",
        app_path.display()
    ))
}
