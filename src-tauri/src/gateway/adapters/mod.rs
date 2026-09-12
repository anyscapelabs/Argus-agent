pub mod anthropic;
pub mod openai_compat;

use reqwest::Client;

use super::schema::{Provider, StreamDone, WireMsg, WireResp};

#[derive(Debug)]
pub struct CallErr {
    pub status: Option<u16>,
    pub msg: String,
    pub retry_after: Option<u64>,
}

impl CallErr {
    pub fn retryable(&self) -> bool {
        match self.status {
            None => true,
            Some(s) => s == 402 || s == 408 || s == 429 || s >= 500,
        }
    }
}

pub(crate) fn retry_after_secs(resp: &reqwest::Response) -> Option<u64> {
    let v = resp
        .headers()
        .get(reqwest::header::RETRY_AFTER)?
        .to_str()
        .ok()?
        .trim()
        .parse()
        .ok()?;
    Some(v)
}

pub fn sse_events(buf: &mut String) -> Vec<String> {
    let mut out = vec![];

    while let Some(pos) = buf.find("\n\n") {
        out.push(buf.drain(..pos + 2).collect());
    }

    out
}

pub type DeltaSink<'a> = &'a mut (dyn FnMut(&str) -> Result<(), String> + Send + 'a);

const SHOT_GUARD: &str = "/screenshots/shot-";

fn shot_bytes(path: &str) -> Option<Vec<u8>> {
    if !path.ends_with(".png") || !path.contains(SHOT_GUARD) {
        return None;
    }

    std::fs::read(path).ok()
}

fn b64(data: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(data)
}

/// OpenAI wire form: plain string content, or text+image parts when the
/// message carries screenshots.
pub fn openai_msgs(msgs: &[WireMsg]) -> Vec<serde_json::Value> {
    msgs.iter()
        .map(|m| {
            if m.images.is_empty() {
                return serde_json::json!({ "role": m.role, "content": m.content });
            }

            let mut parts = vec![serde_json::json!({ "type": "text", "text": m.content })];

            for p in &m.images {
                if let Some(b) = shot_bytes(p) {
                    parts.push(serde_json::json!({
                        "type": "image_url",
                        "image_url": { "url": format!("data:image/png;base64,{}", b64(&b)) }
                    }));
                }
            }

            serde_json::json!({ "role": m.role, "content": parts })
        })
        .collect()
}

/// Anthropic wire form for one message's content.
pub fn anthropic_content(m: &WireMsg) -> serde_json::Value {
    if m.images.is_empty() {
        return serde_json::json!(m.content);
    }

    let mut parts = vec![serde_json::json!({ "type": "text", "text": m.content })];

    for p in &m.images {
        if let Some(b) = shot_bytes(p) {
            parts.push(serde_json::json!({
                "type": "image",
                "source": { "type": "base64", "media_type": "image/png", "data": b64(&b) }
            }));
        }
    }

    serde_json::json!(parts)
}

pub async fn dispatch_stream(
    http: &Client,
    prov: &Provider,
    remote_id: &str,
    tok: Option<String>,
    msgs: &[WireMsg],
    on_delta: DeltaSink<'_>,
) -> Result<StreamDone, CallErr> {
    let lc = prov.compatible.to_lowercase();

    match lc.as_str() {
        "openai" => {
            openai_compat::stream(http, &prov.base_url, tok, remote_id, msgs, on_delta).await
        }
        "anthropic" => {
            anthropic::stream(http, &prov.base_url, tok, remote_id, msgs, on_delta).await
        }
        _ => Err(CallErr {
            status: None,
            msg: format!("unknown compatible dialect {}", prov.compatible),
            retry_after: None,
        }),
    }
}

pub async fn dispatch(
    http: &Client,
    prov: &Provider,
    remote_id: &str,
    tok: Option<String>,
    msgs: &[WireMsg],
) -> Result<(WireResp, String), CallErr> {
    let lc = prov.compatible.to_lowercase();

    match lc.as_str() {
        "openai" => openai_compat::chat(http, &prov.base_url, tok, remote_id, msgs).await,
        "anthropic" => anthropic::chat(http, &prov.base_url, tok, remote_id, msgs).await,
        _ => Err(CallErr {
            status: None,
            msg: format!("unknown compatible dialect {}", prov.compatible),
            retry_after: None,
        }),
    }
}

pub fn is_local(base_url: &str) -> bool {
    let host = base_url
        .trim_start_matches("http://")
        .trim_start_matches("https://");

    host.starts_with("localhost") || host.starts_with("127.0.0.1") || host.starts_with("[::1]")
}

pub async fn verify_key(http: &Client, prov: &Provider, key: &str) -> Result<(), String> {
    let (url, body) = if prov.compatible == "Anthropic" {
        (
            format!("{}/v1/messages", prov.base_url.trim_end_matches('/')),
            serde_json::json!({ "model": "auth-probe", "max_tokens": 1 }),
        )
    } else {
        (
            format!("{}/chat/completions", prov.base_url.trim_end_matches('/')),
            serde_json::json!({ "model": "auth-probe", "messages": [], "max_tokens": 1 }),
        )
    };

    let mut req = http.post(&url).json(&body);

    if prov.compatible == "Anthropic" {
        req = req
            .header("x-api-key", key)
            .header("anthropic-version", "2023-06-01");
    } else {
        req = req.bearer_auth(key);
    }

    match req.send().await {
        Ok(r)
            if r.status() == reqwest::StatusCode::UNAUTHORIZED
                || r.status() == reqwest::StatusCode::FORBIDDEN =>
        {
            Err(format!("{} rejected this API key", prov.name))
        }
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(content: &str, images: Vec<String>) -> WireMsg {
        WireMsg {
            role: "user".into(),
            content: content.into(),
            images,
        }
    }

    #[test]
    fn openai_msgs_attaches_only_real_screenshots() {
        let dir = std::env::temp_dir().join(format!("argus-shot-test-{}", std::process::id()));
        let sub = dir.join("screenshots");
        std::fs::create_dir_all(&sub).unwrap();
        let real = sub.join("shot-t.png");
        std::fs::write(&real, b"pngbytes").unwrap();

        let msgs = vec![
            msg("plain", vec![]),
            msg("with shot", vec![real.to_string_lossy().into_owned()]),
            msg("fake", vec!["/tmp/screenshots/shot-missing.png".into()]),
        ];

        let out = openai_msgs(&msgs);

        assert_eq!(out[0]["content"], "plain");
        assert!(out[1]["content"].is_array());
        assert_eq!(out[1]["content"][0]["type"], "text");
        assert_eq!(out[1]["content"][1]["type"], "image_url");
        let url = out[1]["content"][1]["image_url"]["url"].as_str().unwrap();
        assert!(url.starts_with("data:image/png;base64,"));

        // missing file still serializes, just without an image part
        assert_eq!(out[2]["content"][0]["type"], "text");
        assert_eq!(out[2]["content"].as_array().unwrap().len(), 1);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn anthropic_content_switches_to_parts_with_images() {
        let plain = msg("hello", vec![]);
        assert_eq!(anthropic_content(&plain), serde_json::json!("hello"));

        let m = msg("shot", vec!["/tmp/screenshots/shot-x.png".into()]);
        let v = anthropic_content(&m);
        assert!(v.is_array());
        assert_eq!(v[0]["type"], "text");
        assert_eq!(v.as_array().unwrap().len(), 1);
    }
}
