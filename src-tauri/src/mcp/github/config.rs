use serde::Deserialize;

#[derive(Deserialize, Clone)]
pub struct GithubCfg {
    pub client_id: String,
    pub client_secret: String,
}

pub const SCOPES: &str = "repo workflow read:user";

const ENV_ID: &str = "ARGUS_GITHUB_CLIENT_ID";
const ENV_SECRET: &str = "ARGUS_GITHUB_CLIENT_SECRET";
const APP_FILE: &str = "github-oauth.json";

pub fn load() -> Result<GithubCfg, String> {
    if let Ok(id) = std::env::var(ENV_ID) {
        if !id.trim().is_empty() {
            return Ok(GithubCfg {
                client_id: id,
                client_secret: std::env::var(ENV_SECRET).unwrap_or_default(),
            });
        }
    }

    let app_path = crate::sessions::ext_install::data_dir().join(APP_FILE);
    if let Ok(raw) = std::fs::read_to_string(&app_path) {
        if let Ok(cfg) = serde_json::from_str::<GithubCfg>(&raw) {
            if !cfg.client_id.trim().is_empty() {
                return Ok(cfg);
            }
        }
    }

    let dev_path =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/mcp/github/config.json");
    if let Ok(raw) = std::fs::read_to_string(&dev_path) {
        if let Ok(cfg) = serde_json::from_str::<GithubCfg>(&raw) {
            if !cfg.client_id.trim().is_empty() {
                return Ok(cfg);
            }
        }
    }

    Err(format!(
        "github oauth not configured — copy config.example.json to {} (app data) or set {ENV_ID}",
        app_path.display()
    ))
}
