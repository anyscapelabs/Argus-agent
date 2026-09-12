use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use url::Url;

use super::{authed_get, authed_post};

const BASE: &str = "https://gmail.googleapis.com/gmail/v1/users/me";

fn b64url_decode(s: &str) -> String {
    URL_SAFE_NO_PAD
        .decode(s)
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .unwrap_or_default()
}

fn header(msg: &serde_json::Value, name: &str) -> String {
    msg.get("payload")
        .and_then(|p| p.get("headers"))
        .and_then(|h| h.as_array())
        .and_then(|hs| {
            hs.iter().find(|h| {
                h.get("name")
                    .and_then(|n| n.as_str())
                    .map(|n| n.eq_ignore_ascii_case(name))
                    .unwrap_or(false)
            })
        })
        .and_then(|h| h.get("value"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}

fn body_text(msg: &serde_json::Value) -> String {
    if let Some(data) = msg
        .get("payload")
        .and_then(|p| p.get("body"))
        .and_then(|b| b.get("data"))
        .and_then(|d| d.as_str())
    {
        return b64url_decode(data);
    }

    if let Some(parts) = msg
        .get("payload")
        .and_then(|p| p.get("parts"))
        .and_then(|p| p.as_array())
    {
        for part in parts {
            let mime = part.get("mimeType").and_then(|m| m.as_str()).unwrap_or("");

            if mime.starts_with("text/plain") {
                if let Some(data) = part
                    .get("body")
                    .and_then(|b| b.get("data"))
                    .and_then(|d| d.as_str())
                {
                    return b64url_decode(data);
                }
            }
        }

        for part in parts {
            if let Some(data) = part
                .get("body")
                .and_then(|b| b.get("data"))
                .and_then(|d| d.as_str())
            {
                return b64url_decode(data);
            }
        }
    }

    msg.get("snippet")
        .and_then(|s| s.as_str())
        .unwrap_or("")
        .to_string()
}

pub async fn list_messages(query: &str, max: u64) -> Result<serde_json::Value, String> {
    let mut url = Url::parse(&format!("{BASE}/messages")).map_err(|err| err.to_string())?;
    url.query_pairs_mut()
        .append_pair("q", query)
        .append_pair("maxResults", &max.clamp(1, 50).to_string());
    let v = authed_get(url.as_str()).await?;

    let ids: Vec<String> = v
        .get("messages")
        .and_then(|m| m.as_array())
        .map(|ms| {
            ms.iter()
                .filter_map(|m| m.get("id").and_then(|i| i.as_str()).map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    let mut out = vec![];

    for id in ids {
        let m = authed_get(&format!("{BASE}/messages/{id}?format=full")).await?;

        out.push(serde_json::json!({
            "id": id,
            "subject": header(&m, "subject"),
            "from": header(&m, "from"),
            "date": header(&m, "date"),
            "snippet": m.get("snippet").and_then(|s| s.as_str()).unwrap_or(""),
        }));
    }

    Ok(serde_json::Value::Array(out))
}

pub async fn get_message(id: &str) -> Result<serde_json::Value, String> {
    let m = authed_get(&format!("{BASE}/messages/{id}?format=full")).await?;

    Ok(serde_json::json!({
        "id": id,
        "subject": header(&m, "subject"),
        "from": header(&m, "from"),
        "to": header(&m, "to"),
        "date": header(&m, "date"),
        "body": body_text(&m),
    }))
}

pub async fn send_message(
    to: &str,
    subject: &str,
    body: &str,
) -> Result<serde_json::Value, String> {
    if to.trim().is_empty() {
        return Err("missing to".into());
    }

    let raw = format!(
        "To: {to}\r\nSubject: {subject}\r\nContent-Type: text/plain; charset=utf-8\r\n\r\n{body}"
    );
    let enc = URL_SAFE_NO_PAD.encode(raw.as_bytes());

    authed_post(
        &format!("{BASE}/messages/send"),
        &serde_json::json!({"raw": enc}),
    )
    .await
}
