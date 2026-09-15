use serde::Deserialize;

#[derive(Deserialize, Clone)]
pub struct HaCfg {
    pub base_url: String,
}

const ENV_BASE: &str = "ARGUS_HA_URL";
const APP_FILE: &str = "ha-settings.json";

pub fn load() -> Result<HaCfg, String> {
    if let Ok(base) = std::env::var(ENV_BASE) {
        if !base.trim().is_empty() {
            return Ok(HaCfg { base_url: base });
        }
    }

    let app_path = crate::sessions::ext_install::data_dir().join(APP_FILE);
    if let Ok(raw) = std::fs::read_to_string(&app_path) {
        if let Ok(cfg) = serde_json::from_str::<HaCfg>(&raw) {
            if !cfg.base_url.trim().is_empty() {
                return Ok(cfg);
            }
        }
    }

    let dev_path =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/mcp/ha/config.json");
    if let Ok(raw) = std::fs::read_to_string(&dev_path) {
        if let Ok(cfg) = serde_json::from_str::<HaCfg>(&raw) {
            if !cfg.base_url.trim().is_empty() {
                return Ok(cfg);
            }
        }
    }

    Err(format!(
        "home assistant url not configured — copy config.example.json to {} (app data) or set {ENV_BASE}",
        app_path.display()
    ))
}
