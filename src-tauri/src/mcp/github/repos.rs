use super::{authed_get, authed_post};

const BASE: &str = "https://api.github.com";

fn slim(v: &serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "id": v.get("id"),
        "name": v.get("name"),
        "full_name": v.get("full_name"),
        "private": v.get("private"),
        "default_branch": v.get("default_branch"),
        "updated_at": v.get("updated_at"),
        "html_url": v.get("html_url"),
    })
}

pub async fn list_repos() -> Result<serde_json::Value, String> {
    let v = authed_get(&format!("{BASE}/user/repos?per_page=50&sort=updated")).await?;

    Ok(v.as_array()
        .map(|rs| rs.iter().map(slim).collect())
        .unwrap_or(serde_json::Value::Null))
}

pub async fn get_repo(full_name: &str) -> Result<serde_json::Value, String> {
    if full_name.trim().is_empty() {
        return Err("missing repo".into());
    }

    authed_get(&format!("{BASE}/repos/{full_name}")).await
}

pub async fn list_branches(full_name: &str) -> Result<serde_json::Value, String> {
    if full_name.trim().is_empty() {
        return Err("missing repo".into());
    }

    authed_get(&format!("{BASE}/repos/{full_name}/branches?per_page=30")).await
}

pub async fn create_repo(
    name: &str,
    private: bool,
    description: &str,
) -> Result<serde_json::Value, String> {
    if name.trim().is_empty() {
        return Err("missing name".into());
    }

    authed_post(
        &format!("{BASE}/user/repos"),
        &serde_json::json!({
            "name": name,
            "private": private,
            "description": description,
            "auto_init": true,
        }),
    )
    .await
}
