use crate::mcp::vault;

const BASE: &str = "https://api.figma.com/v1";
const SERVICE: &str = "figma";

async fn call(path: &str) -> Result<serde_json::Value, String> {
    let tok = vault::get(SERVICE)?.ok_or("figma not connected")?;

    reqwest::Client::new()
        .get(format!("{BASE}{path}"))
        .header("X-Figma-Token", tok)
        .send()
        .await
        .map_err(|err| err.to_string())?
        .error_for_status()
        .map_err(|err| err.to_string())?
        .json()
        .await
        .map_err(|err| err.to_string())
}

pub async fn me() -> Result<serde_json::Value, String> {
    call("/me").await
}

pub async fn file_meta(key: &str) -> Result<serde_json::Value, String> {
    if key.trim().is_empty() {
        return Err("missing file key".into());
    }

    let v = call(&format!("/files/{key}")).await?;

    Ok(serde_json::json!({
        "name": v.get("name"),
        "updated": v.get("lastModified"),
        "pages": v.get("document").and_then(|d| d.get("children")).and_then(|c| c.as_array()).map(|cs| {
            cs.iter().map(|p| serde_json::json!({"id": p.get("id"), "name": p.get("name")})).collect::<Vec<_>>()
        }),
    }))
}

pub async fn comments(key: &str) -> Result<serde_json::Value, String> {
    if key.trim().is_empty() {
        return Err("missing file key".into());
    }

    let v = call(&format!("/files/{key}/comments")).await?;

    Ok(v.get("comments")
        .cloned()
        .unwrap_or(serde_json::Value::Null))
}

pub async fn post_comment(key: &str, message: &str) -> Result<serde_json::Value, String> {
    let tok = vault::get(SERVICE)?.ok_or("figma not connected")?;

    if key.trim().is_empty() || message.trim().is_empty() {
        return Err("missing file key or message".into());
    }

    reqwest::Client::new()
        .post(format!("{BASE}/files/{key}/comments"))
        .header("X-Figma-Token", tok)
        .json(&serde_json::json!({"message": message}))
        .send()
        .await
        .map_err(|err| err.to_string())?
        .error_for_status()
        .map_err(|err| err.to_string())?
        .json()
        .await
        .map_err(|err| err.to_string())
}
