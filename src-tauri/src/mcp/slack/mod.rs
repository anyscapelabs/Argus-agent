use crate::mcp::vault;

const BASE: &str = "https://slack.com/api";
const SERVICE: &str = "slack";

async fn call(
    method: reqwest::Method,
    path: &str,
    body: Option<&serde_json::Value>,
) -> Result<serde_json::Value, String> {
    let label = format!("{method} {path}");
    let out = async {
        let tok = vault::get(SERVICE)?.ok_or("slack not connected")?;
        let cli = reqwest::Client::new();
        let mut req = cli
            .request(method, format!("{BASE}{path}"))
            .bearer_auth(tok);

        if let Some(b) = body {
            req = req.json(b);
        }

        let v: serde_json::Value = req
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
                .get("error")
                .and_then(|e| e.as_str())
                .unwrap_or("slack error")
                .into());
        }

        Ok(v)
    }
    .await;
    crate::connectors::log::api("slack", &label, &out);
    out
}

fn slim_channel(v: &serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "id": v.get("id"),
        "name": v.get("name"),
        "is_dm": v.get("is_im"),
        "purpose": v.get("purpose").and_then(|p| p.get("value")),
    })
}

fn slim_msg(v: &serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "ts": v.get("ts"),
        "user": v.get("user"),
        "text": v.get("text"),
        "thread_ts": v.get("thread_ts"),
        "reply_count": v.get("reply_count"),
    })
}

pub async fn channels() -> Result<serde_json::Value, String> {
    let v = call(
        reqwest::Method::GET,
        "/conversations.list?types=public_channel,private_channel,im,mpim&limit=50",
        None,
    )
    .await?;

    Ok(v.get("channels")
        .and_then(|c| c.as_array())
        .map(|cs| cs.iter().map(slim_channel).collect())
        .unwrap_or(serde_json::Value::Null))
}

pub async fn history(channel: &str, limit: u64) -> Result<serde_json::Value, String> {
    if channel.trim().is_empty() {
        return Err("missing channel".into());
    }

    let v = call(
        reqwest::Method::GET,
        &format!(
            "/conversations.history?channel={channel}&limit={}",
            limit.clamp(1, 50)
        ),
        None,
    )
    .await?;

    Ok(v.get("messages")
        .and_then(|m| m.as_array())
        .map(|ms| ms.iter().map(slim_msg).collect())
        .unwrap_or(serde_json::Value::Null))
}

pub async fn replies(channel: &str, ts: &str) -> Result<serde_json::Value, String> {
    if channel.trim().is_empty() || ts.trim().is_empty() {
        return Err("missing channel or ts".into());
    }

    let v = call(
        reqwest::Method::GET,
        &format!("/conversations.replies?channel={channel}&ts={ts}&limit=30"),
        None,
    )
    .await?;

    Ok(v.get("messages")
        .and_then(|m| m.as_array())
        .map(|ms| ms.iter().map(slim_msg).collect())
        .unwrap_or(serde_json::Value::Null))
}

pub async fn send(channel: &str, text: &str, thread_ts: &str) -> Result<serde_json::Value, String> {
    if channel.trim().is_empty() || text.trim().is_empty() {
        return Err("missing channel or text".into());
    }

    let mut body = serde_json::json!({"channel": channel, "text": text});

    if !thread_ts.trim().is_empty() {
        body["thread_ts"] = thread_ts.into();
    }

    call(reqwest::Method::POST, "/chat.postMessage", Some(&body)).await
}

pub async fn users() -> Result<serde_json::Value, String> {
    let v = call(reqwest::Method::GET, "/users.list?limit=50", None).await?;

    Ok(v.get("members")
        .and_then(|m| m.as_array())
        .map(|ms| {
            ms.iter()
                .filter(|m| m.get("deleted").and_then(|d| d.as_bool()) != Some(true))
                .map(|m| {
                    serde_json::json!({
                        "id": m.get("id"),
                        "name": m.get("name"),
                        "real_name": m.get("real_name"),
                    })
                })
                .collect()
        })
        .unwrap_or(serde_json::Value::Null))
}
