use keyring::Entry;
use tokio::sync::Mutex as AsyncMutex;

const SERVICE: &str = "argus-github";
const TOKEN_ACCT: &str = "oauth-token";

static TOKEN: AsyncMutex<Option<String>> = AsyncMutex::const_new(None);
static LOGIN: AsyncMutex<Option<String>> = AsyncMutex::const_new(None);

fn entry() -> Result<Entry, String> {
    Entry::new(SERVICE, TOKEN_ACCT).map_err(|err| err.to_string())
}

pub async fn token() -> Result<Option<String>, String> {
    if let Some(t) = TOKEN.lock().await.clone() {
        return Ok(Some(t));
    }

    match entry()?.get_password() {
        Ok(v) => {
            *TOKEN.lock().await = Some(v.clone());
            Ok(Some(v))
        }
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(err) => Err(err.to_string()),
    }
}

pub async fn save_token(tok: &str) -> Result<(), String> {
    entry()?.set_password(tok).map_err(|err| err.to_string())?;
    *TOKEN.lock().await = Some(tok.to_string());

    Ok(())
}

pub async fn set_login(login: String) {
    *LOGIN.lock().await = Some(login);
}

pub async fn get_login() -> Option<String> {
    LOGIN.lock().await.clone()
}

pub async fn clear() {
    *TOKEN.lock().await = None;
    *LOGIN.lock().await = None;

    if let Ok(e) = entry() {
        let _ = e.delete_credential();
    }
}
