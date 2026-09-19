use keyring::Entry;

const SERVICE: &str = "argus-connector";

fn entry(account: &str) -> Result<Entry, String> {
    Entry::new(SERVICE, account).map_err(|err| err.to_string())
}

pub fn save(service: &str, tok: &str) -> Result<(), String> {
    if service.trim().is_empty() || tok.trim().is_empty() {
        return Err("missing service or token".into());
    }

    entry(service)?
        .set_password(tok)
        .map_err(|err| err.to_string())?;
    crate::connectors::log::event(service, "token_saved", "", "ok");

    Ok(())
}

pub fn get(service: &str) -> Result<Option<String>, String> {
    match entry(service)?.get_password() {
        Ok(v) => Ok(Some(v)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(err) => Err(err.to_string()),
    }
}

pub fn clear(service: &str) -> Result<(), String> {
    match entry(service)?.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(err) => Err(err.to_string()),
    }?;

    crate::connectors::log::event(service, "disconnected", "", "ok");

    Ok(())
}

fn named(service: &str, kind: &str) -> String {
    format!("{service}-{kind}")
}

pub fn save_client(service: &str, id: &str) -> Result<(), String> {
    if service.trim().is_empty() || id.trim().is_empty() {
        return Err("missing service or client id".into());
    }

    entry(&named(service, "client"))?
        .set_password(id)
        .map_err(|err| err.to_string())?;
    crate::connectors::log::event(service, "client_saved", "", "ok");

    Ok(())
}

pub fn get_client(service: &str) -> Result<Option<String>, String> {
    match entry(&named(service, "client"))?.get_password() {
        Ok(v) => Ok(Some(v)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(err) => Err(err.to_string()),
    }
}

pub fn save_secret(service: &str, secret: &str) -> Result<(), String> {
    if service.trim().is_empty() {
        return Err("missing service".into());
    }

    entry(&named(service, "secret"))?
        .set_password(secret)
        .map_err(|err| err.to_string())?;

    Ok(())
}

pub fn get_secret(service: &str) -> Result<Option<String>, String> {
    match entry(&named(service, "secret"))?.get_password() {
        Ok(v) => Ok(Some(v)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(err) => Err(err.to_string()),
    }
}

pub async fn refresh_oauth(
    token_url: &str,
    form: &[(&str, &str)],
) -> Result<(String, u64), String> {
    let v: serde_json::Value = reqwest::Client::new()
        .post(token_url)
        .form(form)
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
        .ok_or("refresh gave no access token")?
        .to_string();
    let secs = v.get("expires_in").and_then(|t| t.as_u64()).unwrap_or(3600);

    Ok((tok, secs))
}

#[tauri::command]
pub fn conn_save_token(service: String, token: String) -> Result<(), String> {
    save(&service, &token)
}

#[tauri::command]
pub fn conn_has_token(service: String) -> Result<bool, String> {
    Ok(get(&service)?.is_some())
}

#[tauri::command]
pub fn conn_has_client(service: String) -> Result<bool, String> {
    Ok(get_client(&service)?.is_some())
}

#[tauri::command]
pub fn conn_remove_token(service: String) -> Result<(), String> {
    clear(&service)
}

#[tauri::command]
pub fn conn_save_client(service: String, client_id: String) -> Result<(), String> {
    save_client(&service, &client_id)
}

#[tauri::command]
pub fn conn_save_secret(service: String, secret: String) -> Result<(), String> {
    save_secret(&service, &secret)
}
