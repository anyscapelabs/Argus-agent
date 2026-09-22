use crate::mcp::vault;

const BASE: &str = "https://api.exa.ai";
const SERVICE: &str = "exa";

async fn post(path: &str, body: &serde_json::Value) -> Result<serde_json::Value, String> {
    let label = format!("POST {path}");
    let out = async {
        let tok = vault::get(SERVICE)?.ok_or("exa not connected")?;

        reqwest::Client::new()
            .post(format!("{BASE}{path}"))
            .header("x-api-key", tok)
            .json(body)
            .send()
            .await
            .map_err(|err| err.to_string())?
            .error_for_status()
            .map_err(|err| err.to_string())?
            .json()
            .await
            .map_err(|err| err.to_string())
    }
    .await;
    crate::connectors::log::api("exa", &label, &out);
    out
}

fn slim_result(v: &serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "title": v.get("title"),
        "url": v.get("url"),
        "published": v.get("publishedDate"),
        "snippet": v.get("text").and_then(|t| t.as_str()).map(|t| t.chars().take(400).collect::<String>()),
    })
}

pub async fn search(query: &str, limit: u64) -> Result<serde_json::Value, String> {
    if query.trim().is_empty() {
        return Err("missing query".into());
    }

    let v = post(
        "/search",
        &serde_json::json!({"query": query, "numResults": limit.clamp(1, 20)}),
    )
    .await?;

    Ok(v.get("results")
        .and_then(|r| r.as_array())
        .map(|rs| rs.iter().map(slim_result).collect())
        .unwrap_or(serde_json::Value::Null))
}

pub async fn contents(url: &str) -> Result<serde_json::Value, String> {
    if url.trim().is_empty() {
        return Err("missing url".into());
    }

    let v = post(
        "/contents",
        &serde_json::json!({"urls": [url], "text": true}),
    )
    .await?;

    Ok(v.get("results")
        .and_then(|r| r.as_array())
        .and_then(|rs| rs.first())
        .map(|r| {
            serde_json::json!({
                "title": r.get("title"),
                "url": r.get("url"),
                "text": r.get("text"),
            })
        })
        .unwrap_or(serde_json::Value::Null))
}
