use crate::mcp::vault;

const SERVICE: &str = "telegram";

async fn call(method: &str, body: &serde_json::Value) -> Result<serde_json::Value, String> {
    let tok = vault::get(SERVICE)?.ok_or("telegram not connected")?;

    let v: serde_json::Value = reqwest::Client::new()
        .post(format!("https://api.telegram.org/bot{tok}/{method}"))
        .json(body)
        .send()
        .await
        .map_err(|err| err.to_string())?
        .error_for_status()
        .map_err(|err| err.to_string())?
        .json()
        .await
        .map_err(|err| err.to_string())?;

    if v.get("ok").and_then(|o| o.as_bool()) == Some(false) {
        return Err(v
            .get("description")
            .and_then(|d| d.as_str())
            .unwrap_or("telegram error")
            .into());
    }

    Ok(v.get("result").cloned().unwrap_or(serde_json::Value::Null))
}

pub async fn me() -> Result<serde_json::Value, String> {
    call("getMe", &serde_json::json!({})).await
}

pub async fn updates() -> Result<serde_json::Value, String> {
    let v = call(
        "getUpdates",
        &serde_json::json!({"limit": 20, "timeout": 0}),
    )
    .await?;

    Ok(v
        .as_array()
        .map(|us| {
            us.iter()
                .map(|u| {
                    serde_json::json!({
                        "id": u.get("update_id"),
                        "chat": u.get("message").and_then(|m| m.get("chat")).and_then(|c| c.get("id")),
                        "from": u.get("message").and_then(|m| m.get("from")).and_then(|f| f.get("username")),
                        "text": u.get("message").and_then(|m| m.get("text")),
                        "date": u.get("message").and_then(|m| m.get("date")),
                    })
                })
                .collect()
        })
        .unwrap_or(serde_json::Value::Null))
}

pub async fn send(chat_id: &str, text: &str) -> Result<serde_json::Value, String> {
    if chat_id.trim().is_empty() || text.trim().is_empty() {
        return Err("missing chat or text".into());
    }

    call(
        "sendMessage",
        &serde_json::json!({"chat_id": chat_id, "text": text}),
    )
    .await
}
