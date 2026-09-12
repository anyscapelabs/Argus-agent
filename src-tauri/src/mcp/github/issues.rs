use super::{authed_get, authed_patch, authed_post};

const BASE: &str = "https://api.github.com";

fn slim(v: &serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "number": v.get("number"),
        "title": v.get("title"),
        "state": v.get("state"),
        "user": v.get("user").and_then(|u| u.get("login")),
        "updated_at": v.get("updated_at"),
        "html_url": v.get("html_url"),
    })
}

pub async fn list_issues(full_name: &str, state: &str) -> Result<serde_json::Value, String> {
    if full_name.trim().is_empty() {
        return Err("missing repo".into());
    }

    let st = if state.trim().is_empty() {
        "open"
    } else {
        state
    };
    let v = authed_get(&format!(
        "{BASE}/repos/{full_name}/issues?state={st}&per_page=30"
    ))
    .await?;

    Ok(v.as_array()
        .map(|is| is.iter().map(slim).collect())
        .unwrap_or(serde_json::Value::Null))
}

pub async fn get_issue(full_name: &str, number: i64) -> Result<serde_json::Value, String> {
    if full_name.trim().is_empty() {
        return Err("missing repo".into());
    }

    authed_get(&format!("{BASE}/repos/{full_name}/issues/{number}")).await
}

pub async fn create_issue(
    full_name: &str,
    title: &str,
    body: &str,
) -> Result<serde_json::Value, String> {
    if full_name.trim().is_empty() {
        return Err("missing repo".into());
    }

    if title.trim().is_empty() {
        return Err("missing title".into());
    }

    authed_post(
        &format!("{BASE}/repos/{full_name}/issues"),
        &serde_json::json!({"title": title, "body": body}),
    )
    .await
}

pub async fn comment_issue(
    full_name: &str,
    number: i64,
    body: &str,
) -> Result<serde_json::Value, String> {
    if full_name.trim().is_empty() {
        return Err("missing repo".into());
    }

    if body.trim().is_empty() {
        return Err("missing body".into());
    }

    authed_post(
        &format!("{BASE}/repos/{full_name}/issues/{number}/comments"),
        &serde_json::json!({"body": body}),
    )
    .await
}

pub async fn close_issue(full_name: &str, number: i64) -> Result<serde_json::Value, String> {
    if full_name.trim().is_empty() {
        return Err("missing repo".into());
    }

    authed_patch(
        &format!("{BASE}/repos/{full_name}/issues/{number}"),
        &serde_json::json!({"state": "closed"}),
    )
    .await
}
