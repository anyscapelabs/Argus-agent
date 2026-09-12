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

pub fn load() -> Result<GoogleCfg, String> {
    if let (Ok(id), Ok(secret)) = (std::env::var(ENV_ID), std::env::var(ENV_SECRET)) {
        if !id.trim().is_empty() {
            return Ok(GoogleCfg {
                client_id: id,
                client_secret: secret,
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
        "google oauth not configured — copy config.example.json to {} (app data) or set {ENV_ID}/{ENV_SECRET}",
        app_path.display()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scopes_cover_all_five_services() {
        let joined = SCOPES.join(" ");

        for s in [
            "gmail.modify",
            "/auth/calendar",
            "/auth/drive",
            "/auth/documents",
            "/auth/spreadsheets",
        ] {
            assert!(joined.contains(s), "missing {s}");
        }
    }

    #[test]
    fn example_config_parses() {
        let raw = include_str!("config.example.json");
        let cfg: GoogleCfg = serde_json::from_str(raw).unwrap();

        assert!(!cfg.client_id.is_empty());
    }
}
