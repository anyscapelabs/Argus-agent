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
        connected: tokens::is_connected().await?,
        login: tokens::get_login().await,
    })
}

#[tauri::command]
pub async fn github_connect() -> Result<GithubDevice, String> {
    let d = device::begin().await?;
    crate::connectors::log::event("github", "oauth_started", "", "ok");

    Ok(d)
}

#[tauri::command]
pub async fn github_disconnect(gw: State<'_, Gateway>) -> Result<(), String> {
    tokens::clear().await;
    crate::connectors::log::event("github", "disconnected", "", "ok");

    let conn = gw.conn.lock().map_err(|err| err.to_string())?;
    crate::gateway::store::kv_set(&conn, "github_login", "")?;

    Ok(())
}

async fn authed(
    method: reqwest::Method,
    url: &str,
    body: Option<&serde_json::Value>,
) -> Result<reqwest::Response, String> {
    let label = format!("{method} {url}");
    let out = async {
        let resp = send_authed(&method, url, body, tokens::access_token().await?).await?;

        if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
            // The cached access token may have expired between calls: drop it
            // and retry once with a refreshed one before giving up.
            tokens::drop_cached_access().await;
            let resp = send_authed(&method, url, body, tokens::access_token().await?).await?;

            if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
                return Err(
                    "github rejected the token (401) — reconnect GitHub in Connectors".to_string(),
                );
            }

            return resp.error_for_status().map_err(|err| err.to_string());
        }

        resp.error_for_status().map_err(|err| err.to_string())
    }
    .await;
    crate::connectors::log::api("github", &label, &out);
    out
}

async fn send_authed(
    method: &reqwest::Method,
    url: &str,
    body: Option<&serde_json::Value>,
    tok: String,
) -> Result<reqwest::Response, String> {
    let mut req = crate::mcp::http_client()
        .request(method.clone(), url)
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
