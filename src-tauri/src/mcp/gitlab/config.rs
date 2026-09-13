use serde::Deserialize;

#[derive(Deserialize, Clone)]
pub struct GitlabCfg {
    pub base_url: String,
}

const ENV_BASE: &str = "ARGUS_GITLAB_URL";
const APP_FILE: &str = "gitlab-settings.json";
const DEF_BASE: &str = "https://gitlab.com";

pub fn load() -> GitlabCfg {
    if let Ok(base) = std::env::var(ENV_BASE) {
        if !base.trim().is_empty() {
            return GitlabCfg { base_url: base };
        }
    }

    let app_path = crate::sessions::ext_install::data_dir().join(APP_FILE);
    if let Ok(raw) = std::fs::read_to_string(&app_path) {
        if let Ok(cfg) = serde_json::from_str::<GitlabCfg>(&raw) {
            if !cfg.base_url.trim().is_empty() {
                return cfg;
            }
        }
    }

    let dev_path =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/mcp/gitlab/config.json");
    if let Ok(raw) = std::fs::read_to_string(&dev_path) {
        if let Ok(cfg) = serde_json::from_str::<GitlabCfg>(&raw) {
            if !cfg.base_url.trim().is_empty() {
                return cfg;
            }
        }
    }

    GitlabCfg {
        base_url: DEF_BASE.into(),
    }
}
