use std::time::{Duration, Instant};

use keyring::Entry;
use tokio::sync::Mutex as AsyncMutex;

const SERVICE: &str = "argus-google";
const REFRESH_ACCT: &str = "oauth-refresh";
const SKEW: Duration = Duration::from_secs(60);

static ACCESS: AsyncMutex<Option<(String, Instant)>> = AsyncMutex::const_new(None);
static EMAIL: AsyncMutex<Option<String>> = AsyncMutex::const_new(None);

fn entry() -> Result<Entry, String> {
    Entry::new(SERVICE, REFRESH_ACCT).map_err(|err| err.to_string())
}

pub fn refresh_token() -> Result<Option<String>, String> {
    match entry()?.get_password() {
        Ok(v) => Ok(Some(v)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(err) => Err(err.to_string()),
    }
}

pub fn save_refresh(tok: &str) -> Result<(), String> {
    entry()?.set_password(tok).map_err(|err| err.to_string())
}

pub async fn set_email(email: String) {
    *EMAIL.lock().await = Some(email);
}

pub async fn get_email() -> Option<String> {
    EMAIL.lock().await.clone()
}

pub async fn access_token() -> Result<String, String> {
    if let Some((tok, exp)) = ACCESS.lock().await.clone() {
        if Instant::now() + SKEW < exp {
            return Ok(tok);
        }
    }

    let refresh = refresh_token()?.ok_or("google not connected")?;
    let cfg = super::config::load()?;
    let cli = reqwest::Client::new();
    let mut form = vec![
        ("grant_type", "refresh_token"),
        ("refresh_token", refresh.as_str()),
        ("client_id", cfg.client_id.as_str()),
    ];

    if !cfg.client_secret.trim().is_empty() {
        form.push(("client_secret", cfg.client_secret.as_str()));
    }

    let v: serde_json::Value = cli
        .post("https://oauth2.googleapis.com/token")
        .form(&form)
        .send()
        .await
        .map_err(|err| err.to_string())?
        .error_for_status()
        .map_err(|err| err.to_string())?
        .json()
        .await
        .map_err(|err| err.to_string())?;

    let tok = v
        .get("access_token")
        .and_then(|t| t.as_str())
        .ok_or("google refresh gave no access token")?
        .to_string();
    let secs = v.get("expires_in").and_then(|t| t.as_u64()).unwrap_or(3600);

    *ACCESS.lock().await = Some((tok.clone(), Instant::now() + Duration::from_secs(secs)));

    Ok(tok)
}

pub async fn clear() {
    *ACCESS.lock().await = None;
    *EMAIL.lock().await = None;

    if let Ok(e) = entry() {
        let _ = e.delete_credential();
    }
}
