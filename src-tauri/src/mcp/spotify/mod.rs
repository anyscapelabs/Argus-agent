pub mod config;
pub mod oauth;

use serde::Serialize;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpotifyStatus {
    pub connected: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpotifyAuthUrl {
    pub url: String,
}

#[tauri::command]
pub async fn spotify_status() -> Result<SpotifyStatus, String> {
    Ok(SpotifyStatus {
        connected: super::vault::get("spotify")?.is_some(),
    })
}

#[tauri::command]
pub async fn spotify_connect_url() -> Result<SpotifyAuthUrl, String> {
    let url = oauth::auth_url().await?;
    crate::connectors::log::event("spotify", "oauth_started", "", "ok");

    Ok(SpotifyAuthUrl { url })
}

#[tauri::command]
pub async fn spotify_disconnect() -> Result<(), String> {
    oauth::clear().await;

    Ok(())
}

async fn authed(
    method: reqwest::Method,
    path: &str,
    query: &[(&str, &str)],
) -> Result<Option<serde_json::Value>, String> {
    let label = format!("{method} {path}");
    let out = async {
        let tok = oauth::access_token().await?;
        let cli = reqwest::Client::new();
        let mut url = url::Url::parse(&format!("https://api.spotify.com/v1{path}"))
            .map_err(|err| err.to_string())?;
        url.query_pairs_mut().extend_pairs(query);

        let resp = cli
            .request(method, url)
            .bearer_auth(tok)
            .send()
            .await
            .map_err(|err| err.to_string())?
            .error_for_status()
            .map_err(|err| err.to_string())?;
        let text = resp.text().await.map_err(|err| err.to_string())?;

        if text.trim().is_empty() {
            return Ok(None);
        }

        serde_json::from_str(&text)
            .map(Some)
            .map_err(|err| err.to_string())
    }
    .await;
    crate::connectors::log::api("spotify", &label, &out);
    out
}

fn slim_track(item: &serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "name": item.get("name"),
        "artists": item.get("artists").and_then(|a| a.as_array()).map(|as_| {
            as_.iter()
                .filter_map(|a| a.get("name").and_then(|n| n.as_str()))
                .collect::<Vec<_>>()
        }),
        "playing": item.get("is_playing"),
        "progress_ms": item.get("progress_ms"),
    })
}

pub async fn now_playing() -> Result<serde_json::Value, String> {
    Ok(
        authed(reqwest::Method::GET, "/me/player/currently-playing", &[])
            .await?
            .and_then(|v| v.get("item").cloned())
            .map(|i| slim_track(&i))
            .unwrap_or(serde_json::Value::Null),
    )
}

pub async fn control(action: &str) -> Result<(), String> {
    let path = match action {
        "play" => "/me/player/play",
        "pause" => "/me/player/pause",
        "next" => "/me/player/next",
        "prev" => "/me/player/previous",
        _ => return Err("action must be play, pause, next, or prev".into()),
    };
    let method = if action == "play" || action == "pause" {
        reqwest::Method::PUT
    } else {
        reqwest::Method::POST
    };

    authed(method, path, &[]).await?;

    Ok(())
}
