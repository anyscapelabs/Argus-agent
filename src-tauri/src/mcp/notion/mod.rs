use crate::mcp::vault;

const BASE: &str = "https://api.notion.com/v1";
const VERSION: &str = "2022-06-28";
const SERVICE: &str = "notion";

async fn call(
    method: reqwest::Method,
    path: &str,
    body: Option<&serde_json::Value>,
) -> Result<serde_json::Value, String> {
    let label = format!("{method} {path}");
    let out = async {
        let tok = vault::get(SERVICE)?.ok_or("notion not connected")?;
        let cli = reqwest::Client::new();
        let mut req = cli
            .request(method, format!("{BASE}{path}"))
            .bearer_auth(tok)
            .header("Notion-Version", VERSION);

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
    .await;
    crate::connectors::log::api("notion", &label, &out);
    out
}

pub async fn search(query: &str) -> Result<serde_json::Value, String> {
    let v = call(
        reqwest::Method::POST,
        "/search",
        Some(&serde_json::json!({"query": query, "page_size": 20})),
    )
    .await?;

    Ok(v.get("results")
        .and_then(|r| r.as_array())
        .map(|rs| {
            rs.iter()
                .map(|r| {
                    serde_json::json!({
                        "id": r.get("id"),
                        "object": r.get("object"),
                        "title": r.get("properties")
                            .and_then(|p| p.as_object())
                            .and_then(|p| p.values().find_map(|v| {
                                v.get("title")
                                    .and_then(|t| t.as_array())
                                    .and_then(|t| t.first())
                                    .and_then(|t| t.get("plain_text"))
                            })),
                    })
                })
                .collect()
        })
        .unwrap_or(serde_json::Value::Null))
}

pub async fn query_db(db_id: &str, page_size: u64) -> Result<serde_json::Value, String> {
    if db_id.trim().is_empty() {
        return Err("missing database".into());
    }

    call(
        reqwest::Method::POST,
        &format!("/databases/{db_id}/query"),
        Some(&serde_json::json!({"page_size": page_size.clamp(1, 50)})),
    )
    .await
}

pub async fn get_page(page_id: &str) -> Result<serde_json::Value, String> {
    if page_id.trim().is_empty() {
        return Err("missing page".into());
    }

    call(reqwest::Method::GET, &format!("/pages/{page_id}"), None).await
}

pub async fn create_page(parent_id: &str, title: &str) -> Result<serde_json::Value, String> {
    if parent_id.trim().is_empty() || title.trim().is_empty() {
        return Err("missing parent or title".into());
    }

    call(
        reqwest::Method::POST,
        "/pages",
        Some(&serde_json::json!({
            "parent": {"page_id": parent_id},
            "properties": {"title": {"title": [{"text": {"content": title}}]}},
        })),
    )
    .await
}

pub async fn read_blocks(block_id: &str) -> Result<serde_json::Value, String> {
    if block_id.trim().is_empty() {
        return Err("missing block".into());
    }

    let v = call(
        reqwest::Method::GET,
        &format!("/blocks/{block_id}/children?page_size=50"),
        None,
    )
    .await?;

    Ok(v.get("results")
        .and_then(|r| r.as_array())
        .map(|rs| {
            rs.iter()
                .map(|b| {
                    serde_json::json!({
                        "id": b.get("id"),
                        "type": b.get("type"),
                        "text": b.as_object().and_then(|o| {
                            o.values().find_map(|v| {
                                v.get("rich_text")
                                    .and_then(|t| t.as_array())
                                    .map(|ts| {
                                        ts.iter()
                                            .filter_map(|t| {
                                                t.get("plain_text").and_then(|p| p.as_str())
                                            })
                                            .collect::<Vec<_>>()
                                            .join("")
                                    })
                            })
                        }),
                    })
                })
                .collect()
        })
        .unwrap_or(serde_json::Value::Null))
}

pub async fn append_text(block_id: &str, text: &str) -> Result<serde_json::Value, String> {
    if block_id.trim().is_empty() || text.trim().is_empty() {
        return Err("missing block or text".into());
    }

    call(
        reqwest::Method::PATCH,
        &format!("/blocks/{block_id}/children"),
        Some(&serde_json::json!({
            "children": [{
                "object": "block",
                "type": "paragraph",
                "paragraph": {"rich_text": [{"type": "text", "text": {"content": text}}]},
            }],
        })),
    )
    .await
}
