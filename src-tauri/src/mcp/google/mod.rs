pub mod calendar;
pub mod config;
pub mod docs;
pub mod drive;
pub mod gmail;
pub mod oauth;
pub mod sheets;
pub mod tokens;

use serde::Serialize;
use tauri::State;

use crate::gateway::Gateway;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GoogleStatus {
    pub connected: bool,
    pub email: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GoogleAuthUrl {
    pub url: String,
}

#[tauri::command]
pub async fn google_status(gw: State<'_, Gateway>) -> Result<GoogleStatus, String> {
    let conn = gw.conn.lock().map_err(|err| err.to_string())?;
    let email = crate::gateway::store::kv_get(&conn, "google_email");
    let has = tokens::refresh_token()?.is_some();

    Ok(GoogleStatus {
        connected: has,
        email,
    })
}

#[tauri::command]
pub async fn google_connect_url() -> Result<GoogleAuthUrl, String> {
    let url = oauth::auth_url().await?;

    Ok(GoogleAuthUrl { url })
}

#[tauri::command]
pub async fn google_disconnect(gw: State<'_, Gateway>) -> Result<(), String> {
    if let Ok(Some(refresh)) = tokens::refresh_token() {
        oauth::revoke(&refresh).await;
    }

    tokens::clear().await;

    let conn = gw.conn.lock().map_err(|err| err.to_string())?;
    crate::gateway::store::kv_set(&conn, "google_email", "")?;

    Ok(())
}

pub async fn authed_get(url: &str) -> Result<serde_json::Value, String> {
    let tok = tokens::access_token().await?;
    let cli = reqwest::Client::new();

    cli.get(url)
        .bearer_auth(tok)
        .send()
        .await
        .map_err(|err| err.to_string())?
        .error_for_status()
        .map_err(|err| err.to_string())?
        .json()
        .await
        .map_err(|err| err.to_string())
}

pub async fn authed_post(url: &str, body: &serde_json::Value) -> Result<serde_json::Value, String> {
    let tok = tokens::access_token().await?;
    let cli = reqwest::Client::new();

    cli.post(url)
        .bearer_auth(tok)
        .json(body)
        .send()
        .await
        .map_err(|err| err.to_string())?
        .error_for_status()
        .map_err(|err| err.to_string())?
        .json()
        .await
        .map_err(|err| err.to_string())
}

pub async fn authed_put(url: &str, body: &serde_json::Value) -> Result<serde_json::Value, String> {
    let tok = tokens::access_token().await?;
    let cli = reqwest::Client::new();

    cli.put(url)
        .bearer_auth(tok)
        .json(body)
        .send()
        .await
        .map_err(|err| err.to_string())?
        .error_for_status()
        .map_err(|err| err.to_string())?
        .json()
        .await
        .map_err(|err| err.to_string())
}

pub async fn authed_delete(url: &str) -> Result<(), String> {
    let tok = tokens::access_token().await?;
    let cli = reqwest::Client::new();

    cli.delete(url)
        .bearer_auth(tok)
        .send()
        .await
        .map_err(|err| err.to_string())?
        .error_for_status()
        .map_err(|err| err.to_string())?;

    Ok(())
}
