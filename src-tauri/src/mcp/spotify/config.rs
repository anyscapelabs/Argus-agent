use serde::Deserialize;

#[derive(Deserialize, Clone)]
pub struct SpotifyCfg {
    pub client_id: String,
}

pub const SCOPES: &str =
    "user-read-playback-state user-modify-playback-state user-read-currently-playing";

const ENV_ID: &str = "ARGUS_SPOTIFY_CLIENT_ID";
const APP_FILE: &str = "spotify-oauth.json";

pub fn load() -> Result<SpotifyCfg, String> {
    if let Ok(id) = std::env::var(ENV_ID) {
        if !id.trim().is_empty() {
            return Ok(SpotifyCfg { client_id: id });
        }
    }

    let app_path = crate::sessions::ext_install::data_dir().join(APP_FILE);
    if let Ok(raw) = std::fs::read_to_string(&app_path) {
        if let Ok(cfg) = serde_json::from_str::<SpotifyCfg>(&raw) {
            if !cfg.client_id.trim().is_empty() {
                return Ok(cfg);
            }
        }
    }

    let dev_path =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/mcp/spotify/config.json");
    if let Ok(raw) = std::fs::read_to_string(&dev_path) {
        if let Ok(cfg) = serde_json::from_str::<SpotifyCfg>(&raw) {
            if !cfg.client_id.trim().is_empty() {
                return Ok(cfg);
            }
        }
    }

    Err(format!(
        "spotify not configured — register an app at developer.spotify.com, copy config.example.json to {} (app data) or set {ENV_ID}",
        app_path.display()
    ))
}
