use super::{authed_get, authed_post, authed_put};

const BASE: &str = "https://api.github.com";

fn slim(v: &serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "number": v.get("number"),
        "title": v.get("title"),
        "state": v.get("state"),
        "draft": v.get("draft"),
        "user": v.get("user").and_then(|u| u.get("login")),
        "updated_at": v.get("updated_at"),
        "html_url": v.get("html_url"),
    })
}

pub async fn list_prs(full_name: &str, state: &str) -> Result<serde_json::Value, String> {
    if full_name.trim().is_empty() {
        return Err("missing repo".into());
    }

    let st = if state.trim().is_empty() {
        "open"
    } else {
        state
    };
    let v = authed_get(&format!(
        "{BASE}/repos/{full_name}/pulls?state={st}&per_page=30"
    ))
    .await?;

    Ok(v.as_array()
        .map(|ps| ps.iter().map(slim).collect())
        .unwrap_or(serde_json::Value::Null))
}

pub async fn get_pr(full_name: &str, number: i64) -> Result<serde_json::Value, String> {
    if full_name.trim().is_empty() {
        return Err("missing repo".into());
    }

    authed_get(&format!("{BASE}/repos/{full_name}/pulls/{number}")).await
}

pub async fn create_pr(
    full_name: &str,
    title: &str,
    head: &str,
    base: &str,
    body: &str,
) -> Result<serde_json::Value, String> {
    if full_name.trim().is_empty() {
        return Err("missing repo".into());
    }

    if title.trim().is_empty() || head.trim().is_empty() || base.trim().is_empty() {
        return Err("missing title, head, or base".into());
    }

    authed_post(
        &format!("{BASE}/repos/{full_name}/pulls"),
        &serde_json::json!({"title": title, "head": head, "base": base, "body": body}),
    )
    .await
}

pub async fn merge_pr(full_name: &str, number: i64) -> Result<serde_json::Value, String> {
    if full_name.trim().is_empty() {
        return Err("missing repo".into());
    }

    authed_put(
        &format!("{BASE}/repos/{full_name}/pulls/{number}/merge"),
        &serde_json::json!({"merge_method": "squash"}),
    )
    .await
}
