use crate::mcp::vault;

const BASE: &str = "https://api.linear.app/graphql";
const SERVICE: &str = "linear";

async fn gql(query: &str, vars: &serde_json::Value) -> Result<serde_json::Value, String> {
    let tok = vault::get(SERVICE)?.ok_or("linear not connected")?;

    let v: serde_json::Value = reqwest::Client::new()
        .post(BASE)
        .bearer_auth(tok)
        .json(&serde_json::json!({"query": query, "variables": vars}))
        .send()
        .await
        .map_err(|err| err.to_string())?
        .error_for_status()
        .map_err(|err| err.to_string())?
        .json()
        .await
        .map_err(|err| err.to_string())?;

    if let Some(errs) = v.get("errors").and_then(|e| e.as_array()) {
        if let Some(first) = errs.first() {
            return Err(first
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("linear error")
                .into());
        }
    }

    Ok(v.get("data").cloned().unwrap_or(serde_json::Value::Null))
}

fn slim_issue(v: &serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "id": v.get("id"),
        "identifier": v.get("identifier"),
        "title": v.get("title"),
        "state": v.get("state").and_then(|s| s.get("name")),
        "priority": v.get("priority"),
        "assignee": v.get("assignee").and_then(|a| a.get("name")),
        "url": v.get("url"),
    })
}

pub async fn issues(team_id: &str, limit: u64) -> Result<serde_json::Value, String> {
    let mut filter = serde_json::json!({});
    if !team_id.trim().is_empty() {
        filter["team"] = serde_json::json!({"id": {"eq": team_id}});
    }

    let data = gql(
        "query($first: Int, $filter: IssueFilter) { issues(first: $first, filter: $filter, orderBy: updatedAt) { nodes { id identifier title state { name } priority assignee { name } url } } }",
        &serde_json::json!({"first": limit.clamp(1, 50) as i64, "filter": filter}),
    )
    .await?;

    Ok(data
        .get("issues")
        .and_then(|i| i.get("nodes"))
        .and_then(|n| n.as_array())
        .map(|ns| ns.iter().map(slim_issue).collect())
        .unwrap_or(serde_json::Value::Null))
}

pub async fn get_issue(id: &str) -> Result<serde_json::Value, String> {
    if id.trim().is_empty() {
        return Err("missing issue".into());
    }

    let data = gql(
        "query($id: String!) { issue(id: $id) { id identifier title description state { name } priority assignee { name } url comments(first: 10) { nodes { body user { name } } } } }",
        &serde_json::json!({"id": id}),
    )
    .await?;

    Ok(data
        .get("issue")
        .cloned()
        .unwrap_or(serde_json::Value::Null))
}

pub async fn create_issue(
    team_id: &str,
    title: &str,
    description: &str,
) -> Result<serde_json::Value, String> {
    if team_id.trim().is_empty() || title.trim().is_empty() {
        return Err("missing team or title".into());
    }

    let data = gql(
        "mutation($input: IssueCreateInput!) { issueCreate(input: $input) { issue { id identifier title url } } }",
        &serde_json::json!({"input": {"teamId": team_id, "title": title, "description": description}}),
    )
    .await?;

    Ok(data
        .get("issueCreate")
        .and_then(|c| c.get("issue"))
        .cloned()
        .unwrap_or(serde_json::Value::Null))
}

pub async fn comment_issue(issue_id: &str, body: &str) -> Result<serde_json::Value, String> {
    if issue_id.trim().is_empty() || body.trim().is_empty() {
        return Err("missing issue or body".into());
    }

    let data = gql(
        "mutation($input: CommentCreateInput!) { commentCreate(input: $input) { comment { id body } } }",
        &serde_json::json!({"input": {"issueId": issue_id, "body": body}}),
    )
    .await?;

    Ok(data
        .get("commentCreate")
        .and_then(|c| c.get("comment"))
        .cloned()
        .unwrap_or(serde_json::Value::Null))
}

pub async fn teams() -> Result<serde_json::Value, String> {
    let data = gql(
        "{ teams { nodes { id key name } } }",
        &serde_json::json!({}),
    )
    .await?;

    Ok(data
        .get("teams")
        .and_then(|t| t.get("nodes"))
        .cloned()
        .unwrap_or(serde_json::Value::Null))
}
