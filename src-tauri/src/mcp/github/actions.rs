use super::{authed_get, authed_post};

const BASE: &str = "https://api.github.com";

fn slim(v: &serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "id": v.get("id"),
        "name": v.get("name"),
        "status": v.get("status"),
        "conclusion": v.get("conclusion"),
        "branch": v.get("head_branch"),
        "event": v.get("event"),
        "created_at": v.get("created_at"),
        "html_url": v.get("html_url"),
    })
}

pub async fn list_runs(full_name: &str) -> Result<serde_json::Value, String> {
    if full_name.trim().is_empty() {
        return Err("missing repo".into());
    }

    let v = authed_get(&format!(
        "{BASE}/repos/{full_name}/actions/runs?per_page=20"
    ))
    .await?;

    Ok(v.get("workflow_runs")
        .and_then(|r| r.as_array())
        .map(|rs| rs.iter().map(slim).collect())
        .unwrap_or(serde_json::Value::Null))
}

pub async fn rerun_failed(full_name: &str, run_id: i64) -> Result<(), String> {
    if full_name.trim().is_empty() {
        return Err("missing repo".into());
    }

    authed_post(
        &format!("{BASE}/repos/{full_name}/actions/runs/{run_id}/rerun-failed-jobs"),
        &serde_json::json!({}),
    )
    .await?;

    Ok(())
}
