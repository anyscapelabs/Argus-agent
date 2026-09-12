use super::oauth::url_encode;
use super::{authed_get, authed_post};

const BASE: &str = "https://docs.googleapis.com/v1/documents";

fn para_text(el: &serde_json::Value) -> String {
    el.get("paragraph")
        .and_then(|p| p.get("elements"))
        .and_then(|e| e.as_array())
        .map(|es| {
            es.iter()
                .filter_map(|e| {
                    e.get("textRun")
                        .and_then(|t| t.get("content"))
                        .and_then(|c| c.as_str())
                })
                .collect::<String>()
        })
        .unwrap_or_default()
}

pub async fn get_doc(doc_id: &str) -> Result<serde_json::Value, String> {
    if doc_id.trim().is_empty() {
        return Err("missing doc id".into());
    }

    let d = authed_get(&format!("{BASE}/{}", url_encode(doc_id))).await?;

    let text: String = d
        .get("body")
        .and_then(|b| b.get("content"))
        .and_then(|c| c.as_array())
        .map(|cs| cs.iter().map(para_text).collect())
        .unwrap_or_default();

    Ok(serde_json::json!({
        "id": doc_id,
        "title": d.get("title").and_then(|t| t.as_str()).unwrap_or(""),
        "text": text,
    }))
}

pub async fn create_doc(title: &str) -> Result<serde_json::Value, String> {
    if title.trim().is_empty() {
        return Err("missing title".into());
    }

    authed_post(BASE, &serde_json::json!({"title": title})).await
}

pub async fn append_text(doc_id: &str, text: &str) -> Result<serde_json::Value, String> {
    if doc_id.trim().is_empty() {
        return Err("missing doc id".into());
    }

    if text.is_empty() {
        return Err("missing text".into());
    }

    let d = authed_get(&format!("{BASE}/{}", url_encode(doc_id))).await?;
    let end = d
        .get("body")
        .and_then(|b| b.get("content"))
        .and_then(|c| c.as_array())
        .and_then(|cs| cs.last())
        .and_then(|l| l.get("endIndex"))
        .and_then(|i| i.as_u64())
        .unwrap_or(1)
        .max(1) as i64
        - 1;

    authed_post(
        &format!("{BASE}/{}:batchUpdate", url_encode(doc_id)),
        &serde_json::json!({
            "requests": [{"insertText": {"location": {"index": end}, "text": text}}],
        }),
    )
    .await
}
