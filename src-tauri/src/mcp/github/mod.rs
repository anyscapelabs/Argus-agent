pub mod actions;
pub mod config;
pub mod device;
pub mod issues;
pub mod prs;
pub mod repos;
pub mod tokens;

use serde::Serialize;
use tauri::State;

use crate::gateway::Gateway;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GithubStatus {
    pub connected: bool,
    pub login: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GithubDevice {
    pub verification_uri: String,
    pub user_code: String,
}

#[tauri::command]
pub async fn github_status() -> Result<GithubStatus, String> {
    Ok(GithubStatus {
        connected: tokens::token().await?.is_some(),
        login: tokens::get_login().await,
    })
}

#[tauri::command]
pub async fn github_connect() -> Result<GithubDevice, String> {
    device::begin().await
}

#[tauri::command]
pub async fn github_disconnect(gw: State<'_, Gateway>) -> Result<(), String> {
    tokens::clear().await;

    let conn = gw.conn.lock().map_err(|err| err.to_string())?;
    crate::gateway::store::kv_set(&conn, "github_login", "")?;

    Ok(())
}

async fn authed(
    method: reqwest::Method,
    url: &str,
    body: Option<&serde_json::Value>,
) -> Result<reqwest::Response, String> {
    let tok = tokens::token().await?.ok_or("github not connected")?;
    let cli = reqwest::Client::new();
    let mut req = cli
        .request(method, url)
        .bearer_auth(tok)
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28");

    if let Some(b) = body {
        req = req.json(b);
    }

    req.send().await.map_err(|err| err.to_string())
}

pub async fn authed_get(url: &str) -> Result<serde_json::Value, String> {
    authed(reqwest::Method::GET, url, None)
        .await?
        .error_for_status()
        .map_err(|err| err.to_string())?
        .json()
        .await
        .map_err(|err| err.to_string())
}

pub async fn authed_post(url: &str, body: &serde_json::Value) -> Result<serde_json::Value, String> {
    authed(reqwest::Method::POST, url, Some(body))
        .await?
        .error_for_status()
        .map_err(|err| err.to_string())?
        .json()
        .await
        .map_err(|err| err.to_string())
}

pub async fn authed_patch(
    url: &str,
    body: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    authed(reqwest::Method::PATCH, url, Some(body))
        .await?
        .error_for_status()
        .map_err(|err| err.to_string())?
        .json()
        .await
        .map_err(|err| err.to_string())
}

pub async fn authed_put(url: &str, body: &serde_json::Value) -> Result<serde_json::Value, String> {
    authed(reqwest::Method::PUT, url, Some(body))
        .await?
        .error_for_status()
        .map_err(|err| err.to_string())?
        .json()
        .await
        .map_err(|err| err.to_string())
}
