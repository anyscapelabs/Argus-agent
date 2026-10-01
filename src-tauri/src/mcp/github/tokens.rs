use std::time::{Duration, Instant};

use keyring::Entry;
use tokio::sync::Mutex as AsyncMutex;

const SERVICE: &str = "argus-github";
const TOKEN_ACCT: &str = "oauth-token";
const REFRESH_ACCT: &str = "oauth-refresh";
const LOGIN_ACCT: &str = "oauth-login";
const TOKEN_URL: &str = "https://github.com/login/oauth/access_token";
const SKEW: Duration = Duration::from_secs(60);

/// Cached access token with its expiry. `None` expiry means the OAuth App
/// does not expire user tokens, so the access token is permanent.
static ACCESS: AsyncMutex<Option<(String, Option<Instant>)>> = AsyncMutex::const_new(None);
static LOGIN: AsyncMutex<Option<String>> = AsyncMutex::const_new(None);

fn entry(acct: &str) -> Result<Entry, String> {
    Entry::new(SERVICE, acct).map_err(|err| err.to_string())
}

fn stored(acct: &str) -> Result<Option<String>, String> {
    match entry(acct)?.get_password() {
        Ok(v) => Ok(Some(v)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(err) => Err(err.to_string()),
    }
}

fn fresh(cached: &Option<(String, Option<Instant>)>) -> Option<String> {
    let (tok, exp) = cached.clone()?;
    match exp {
        Some(exp) if Instant::now() + SKEW >= exp => None,
        _ => Some(tok),
    }
}

/// Raw stored access token, no network. Used for `connected` checks.
pub async fn token() -> Result<Option<String>, String> {
    if let Some(t) = fresh(&ACCESS.lock().await.clone()) {
        return Ok(Some(t));
    }

    match stored(TOKEN_ACCT)? {
        Some(v) => {
            *ACCESS.lock().await = Some((v.clone(), None));
            Ok(Some(v))
        }
        None => Ok(None),
    }
}

pub async fn is_connected() -> Result<bool, String> {
    if token().await?.is_some() {
        return Ok(true);
    }

    Ok(refresh_token()?.is_some())
}

fn refresh_token() -> Result<Option<String>, String> {
    stored(REFRESH_ACCT)
}

/// Refresh-aware access token. Permanent (non-expiring) app tokens come
/// straight from the keyring; expiring ones (8h access, rotating refresh) are
/// renewed silently so the user is not asked to reconnect every day.
pub async fn access_token() -> Result<String, String> {
    if let Some(t) = fresh(&ACCESS.lock().await.clone()) {
        return Ok(t);
    }

    if stored(REFRESH_ACCT)?.is_none() {
        // No refresh token: either a permanent app token or nothing at all.
        return stored(TOKEN_ACCT)?.ok_or_else(|| "github not connected".to_string());
    }

    refresh().await
}

async fn refresh() -> Result<String, String> {
    let refresh = refresh_token()?.ok_or_else(|| "github not connected".to_string())?;
    let cfg = super::config::load()?;

    let v: serde_json::Value = reqwest::Client::new()
        .post(TOKEN_URL)
        .header("Accept", "application/json")
        .form(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh.as_str()),
            ("client_id", cfg.client_id.as_str()),
            ("client_secret", cfg.client_secret.as_str()),
        ])
        .send()
        .await
        .map_err(|err| err.to_string())?
        .error_for_status()
        .map_err(|_| "github session expired — reconnect GitHub in Connectors".to_string())?
        .json()
        .await
        .map_err(|err| err.to_string())?;

    let tok = v
        .get("access_token")
        .and_then(|t| t.as_str())
        .ok_or("github refresh gave no access token")?
        .to_string();
    // GitHub rotates the refresh token on every refresh.
    let next_refresh = v.get("refresh_token").and_then(|t| t.as_str());
    let secs = v.get("expires_in").and_then(|t| t.as_u64());

    save_tokens(&tok, next_refresh, secs).await?;

    Ok(tok)
}

/// Drops the cached access token so the next `access_token()` re-reads the
/// keyring or refreshes. Used for one 401 retry, never deletes stored creds.
pub async fn drop_cached_access() {
    *ACCESS.lock().await = None;
}

pub async fn save_tokens(
    access: &str,
    refresh: Option<&str>,
    expires_in: Option<u64>,
) -> Result<(), String> {
    entry(TOKEN_ACCT)?
        .set_password(access)
        .map_err(|err| err.to_string())?;

    match refresh {
        Some(r) if !r.trim().is_empty() => entry(REFRESH_ACCT)?
            .set_password(r)
            .map_err(|err| err.to_string())?,
        // A poll without a refresh token means the app issues permanent
        // tokens: drop any stale refresh so expiry logic stays off.
        _ => {
            let _ = entry(REFRESH_ACCT)?.delete_credential();
        }
    }

    let exp = expires_in.map(|s| Instant::now() + Duration::from_secs(s));
    *ACCESS.lock().await = Some((access.to_string(), exp));

    Ok(())
}

pub async fn save_token(tok: &str) -> Result<(), String> {
    save_tokens(tok, None, None).await
}

pub async fn set_login(login: String) {
    *LOGIN.lock().await = Some(login.clone());

    if let Ok(e) = entry(LOGIN_ACCT) {
        let _ = e.set_password(&login);
    }
}

/// In-memory first, keyring fallback — the login must survive restarts, or
/// the card forgets who is connected.
pub async fn get_login() -> Option<String> {
    if let Some(l) = LOGIN.lock().await.clone() {
        return Some(l);
    }

    match stored(LOGIN_ACCT).unwrap_or(None) {
        Some(l) => {
            *LOGIN.lock().await = Some(l.clone());
            Some(l)
        }
        None => None,
    }
}

pub async fn clear() {
    *ACCESS.lock().await = None;
    *LOGIN.lock().await = None;

    for acct in [TOKEN_ACCT, REFRESH_ACCT, LOGIN_ACCT] {
        if let Ok(e) = entry(acct) {
            let _ = e.delete_credential();
        }
    }
}
