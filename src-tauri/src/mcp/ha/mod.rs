pub mod config;

use crate::mcp::vault;

const SERVICE: &str = "ha";

fn base() -> Result<String, String> {
    Ok(config::load()?.base_url.trim_end_matches('/').to_string())
}

async fn call(
    method: reqwest::Method,
    path: &str,
    body: Option<&serde_json::Value>,
) -> Result<serde_json::Value, String> {
    let tok = vault::get(SERVICE)?.ok_or("home assistant not connected")?;
    let cli = reqwest::Client::new();
    let mut req = cli
        .request(method, format!("{}{path}", base()?))
        .bearer_auth(tok);

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

fn slim(v: &serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "id": v.get("entity_id"),
        "state": v.get("state"),
        "name": v.get("attributes").and_then(|a| a.get("friendly_name")),
        "unit": v.get("attributes").and_then(|a| a.get("unit_of_measurement")),
    })
}

pub async fn states() -> Result<serde_json::Value, String> {
    let v = call(reqwest::Method::GET, "/api/states", None).await?;

    Ok(v.as_array()
        .map(|ss| ss.iter().map(slim).collect())
        .unwrap_or(serde_json::Value::Null))
}

pub async fn get_state(entity_id: &str) -> Result<serde_json::Value, String> {
    if entity_id.trim().is_empty() {
        return Err("missing entity".into());
    }

    call(
        reqwest::Method::GET,
        &format!("/api/states/{entity_id}"),
        None,
    )
    .await
}

pub async fn turn(
    domain: &str,
    service: &str,
    entity_id: &str,
) -> Result<serde_json::Value, String> {
    if domain.trim().is_empty() || service.trim().is_empty() || entity_id.trim().is_empty() {
        return Err("missing domain, service, or entity".into());
    }

    call(
        reqwest::Method::POST,
        &format!("/api/services/{domain}/{service}"),
        Some(&serde_json::json!({"entity_id": entity_id})),
    )
    .await
}

pub async fn config() -> Result<serde_json::Value, String> {
    call(reqwest::Method::GET, "/api/config", None).await
}
