use crate::mcp::vault;

const BASE: &str = "https://discord.com/api/v10";
const SERVICE: &str = "discord";

async fn call(
    method: reqwest::Method,
    path: &str,
    body: Option<&serde_json::Value>,
) -> Result<serde_json::Value, String> {
    let label = format!("{method} {path}");
    let out = async {
        let tok = vault::get(SERVICE)?.ok_or("discord not connected")?;
        let cli = reqwest::Client::new();
        let mut req = cli
            .request(method, format!("{BASE}{path}"))
            .header("Authorization", format!("Bot {tok}"));

        if let Some(b) = body {
            req = req.json(b);
        }

        req.send()
            .await
            .map_err(|err| err.to_string())?
            .error_for_status()
            .map_err(|err| err.to_string())?
            .json()
            .await
            .map_err(|err| err.to_string())
    }
    .await;
    crate::connectors::log::api("discord", &label, &out);
    out
}

pub async fn guilds() -> Result<serde_json::Value, String> {
    let v = call(reqwest::Method::GET, "/users/@me/guilds?limit=50", None).await?;

    Ok(v.as_array()
        .map(|gs| {
            gs.iter()
                .map(|g| serde_json::json!({"id": g.get("id"), "name": g.get("name")}))
                .collect()
        })
        .unwrap_or(serde_json::Value::Null))
}

pub async fn channels(guild_id: &str) -> Result<serde_json::Value, String> {
    if guild_id.trim().is_empty() {
        return Err("missing guild".into());
    }

    let v = call(
        reqwest::Method::GET,
        &format!("/guilds/{guild_id}/channels"),
        None,
    )
    .await?;

    Ok(v.as_array()
        .map(|cs| {
            cs.iter()
                .filter(|c| c.get("type").and_then(|t| t.as_u64()) == Some(0))
                .map(|c| serde_json::json!({"id": c.get("id"), "name": c.get("name")}))
                .collect()
        })
        .unwrap_or(serde_json::Value::Null))
}

pub async fn history(channel_id: &str, limit: u64) -> Result<serde_json::Value, String> {
    if channel_id.trim().is_empty() {
        return Err("missing channel".into());
    }

    let v = call(
        reqwest::Method::GET,
        &format!(
            "/channels/{channel_id}/messages?limit={}",
            limit.clamp(1, 50)
        ),
        None,
    )
    .await?;

    Ok(v.as_array()
        .map(|ms| {
            ms.iter()
                .map(|m| {
                    serde_json::json!({
                        "id": m.get("id"),
                        "author": m.get("author").and_then(|a| a.get("username")),
                        "content": m.get("content"),
                        "timestamp": m.get("timestamp"),
                    })
                })
                .collect()
        })
        .unwrap_or(serde_json::Value::Null))
}

pub async fn send(channel_id: &str, content: &str) -> Result<serde_json::Value, String> {
    if channel_id.trim().is_empty() || content.trim().is_empty() {
        return Err("missing channel or content".into());
    }

    call(
        reqwest::Method::POST,
        &format!("/channels/{channel_id}/messages"),
        Some(&serde_json::json!({"content": content})),
    )
    .await
}
