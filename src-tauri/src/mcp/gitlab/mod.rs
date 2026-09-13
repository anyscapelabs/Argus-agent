pub mod config;

use crate::mcp::vault;

const SERVICE: &str = "gitlab";

fn base() -> String {
    config::load().base_url.trim_end_matches('/').to_string()
}

async fn call(
    method: reqwest::Method,
    path: &str,
    body: Option<&serde_json::Value>,
) -> Result<serde_json::Value, String> {
    let tok = vault::get(SERVICE)?.ok_or("gitlab not connected")?;
    let cli = reqwest::Client::new();
    let mut req = cli
        .request(method, format!("{}/api/v4{path}", base()))
        .header("PRIVATE-TOKEN", tok);

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

fn slim_project(v: &serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "id": v.get("id"),
        "name": v.get("name_with_namespace"),
        "branch": v.get("default_branch"),
        "updated": v.get("last_activity_at"),
        "url": v.get("web_url"),
    })
}

pub async fn projects() -> Result<serde_json::Value, String> {
    let v = call(
        reqwest::Method::GET,
        "/projects?membership=true&per_page=30&order_by=last_activity_at",
        None,
    )
    .await?;

    Ok(v.as_array()
        .map(|ps| ps.iter().map(slim_project).collect())
        .unwrap_or(serde_json::Value::Null))
}

pub async fn issues(project_id: &str) -> Result<serde_json::Value, String> {
    if project_id.trim().is_empty() {
        return Err("missing project".into());
    }

    call(
        reqwest::Method::GET,
        &format!("/projects/{project_id}/issues?per_page=30&order_by=updated_at"),
        None,
    )
    .await
}

pub async fn merge_requests(project_id: &str) -> Result<serde_json::Value, String> {
    if project_id.trim().is_empty() {
        return Err("missing project".into());
    }

    call(
        reqwest::Method::GET,
        &format!("/projects/{project_id}/merge_requests?per_page=30&order_by=updated_at"),
        None,
    )
    .await
}

pub async fn pipelines(project_id: &str) -> Result<serde_json::Value, String> {
    if project_id.trim().is_empty() {
        return Err("missing project".into());
    }

    call(
        reqwest::Method::GET,
        &format!("/projects/{project_id}/pipelines?per_page=10"),
        None,
    )
    .await
}

pub async fn create_issue(
    project_id: &str,
    title: &str,
    description: &str,
) -> Result<serde_json::Value, String> {
    if project_id.trim().is_empty() || title.trim().is_empty() {
        return Err("missing project or title".into());
    }

    call(
        reqwest::Method::POST,
        &format!("/projects/{project_id}/issues"),
        Some(&serde_json::json!({"title": title, "description": description})),
    )
    .await
}
