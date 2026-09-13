use crate::mcp::vault;

const BASE: &str = "https://api.todoist.com/api/v1";
const SERVICE: &str = "todoist";

async fn call(
    method: reqwest::Method,
    path: &str,
    body: Option<&serde_json::Value>,
) -> Result<Option<serde_json::Value>, String> {
    let tok = vault::get(SERVICE)?.ok_or("todoist not connected")?;
    let cli = reqwest::Client::new();
    let mut req = cli
        .request(method, format!("{BASE}{path}"))
        .bearer_auth(tok);

    if let Some(b) = body {
        req = req.json(b);
    }

    let resp = req
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

fn slim(v: &serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "id": v.get("id"),
        "content": v.get("content"),
        "description": v.get("description"),
        "priority": v.get("priority"),
        "due": v.get("due"),
        "labels": v.get("labels"),
        "project_id": v.get("project_id"),
    })
}

pub async fn tasks(filter: &str) -> Result<serde_json::Value, String> {
    let path = if filter.trim().is_empty() {
        "/tasks?limit=30".to_string()
    } else {
        let q = percent_encoding::utf8_percent_encode(filter, percent_encoding::NON_ALPHANUMERIC);
        format!("/tasks?limit=30&filter={q}")
    };

    let v = call(reqwest::Method::GET, &path, None)
        .await?
        .unwrap_or(serde_json::Value::Null);

    Ok(v.get("results")
        .and_then(|r| r.as_array())
        .map(|ts| ts.iter().map(slim).collect())
        .unwrap_or(v))
}

pub async fn create_task(
    content: &str,
    description: &str,
    priority: u8,
) -> Result<serde_json::Value, String> {
    if content.trim().is_empty() {
        return Err("missing content".into());
    }

    call(
        reqwest::Method::POST,
        "/tasks",
        Some(&serde_json::json!({
            "content": content,
            "description": description,
            "priority": priority.clamp(1, 4),
        })),
    )
    .await?
    .ok_or("empty response".into())
}

pub async fn close_task(id: &str) -> Result<(), String> {
    if id.trim().is_empty() {
        return Err("missing task".into());
    }

    call(reqwest::Method::POST, &format!("/tasks/{id}/close"), None).await?;

    Ok(())
}

pub async fn delete_task(id: &str) -> Result<(), String> {
    if id.trim().is_empty() {
        return Err("missing task".into());
    }

    call(reqwest::Method::DELETE, &format!("/tasks/{id}"), None).await?;

    Ok(())
}
