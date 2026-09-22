use serde::Deserialize;

#[derive(Deserialize, Clone)]
pub struct TrelloCfg {
    pub api_key: String,
}

const ENV_KEY: &str = "ARGUS_TRELLO_KEY";
const APP_FILE: &str = "trello-settings.json";

pub fn load() -> Result<TrelloCfg, String> {
    if let Ok(Some(key)) = crate::mcp::vault::get_client("trello") {
        if !key.trim().is_empty() {
            return Ok(TrelloCfg { api_key: key });
        }
    }

    if let Ok(key) = std::env::var(ENV_KEY) {
        if !key.trim().is_empty() {
            return Ok(TrelloCfg { api_key: key });
        }
    }

    let app_path = crate::sessions::ext_install::data_dir().join(APP_FILE);
    if let Ok(raw) = std::fs::read_to_string(&app_path) {
        match serde_json::from_str::<TrelloCfg>(&raw) {
            Ok(cfg) if !cfg.api_key.trim().is_empty() => return Ok(cfg),
            Ok(_) => {
                return Err(format!("{}: api_key is empty", app_path.display()));
            }
            Err(err) => {
                return Err(format!(
                    "{}: couldn't parse — expected {{\"api_key\": \"...\"}}, got: {err}",
                    app_path.display()
                ));
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
