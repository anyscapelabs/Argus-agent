pub mod anthropic;
pub mod openai_compat;

use reqwest::Client;

use super::schema::{Provider, StreamDone, ToolSpec, WireMsg, WireResp};

#[derive(Debug)]
pub struct CallError {
    pub status: Option<u16>,
    pub msg: String,
    pub retry_after: Option<u64>,
}

impl CallError {
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

pub fn wire_name(name: &str) -> String {
    name.replace('.', "_")
}

pub fn real_name(wire: &str, tools: &[ToolSpec]) -> String {
    if tools.iter().any(|t| t.name == wire) {
        return wire.to_string();
    }

    tools
        .iter()
        .find(|t| wire_name(&t.name) == wire)
        .map(|t| t.name.clone())
        .unwrap_or_else(|| wire.to_string())
}

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

pub fn openai_msgs(msgs: &[WireMsg]) -> Vec<serde_json::Value> {
    msgs.iter()
        .map(|m| {
            if m.role == "tool" {
                return serde_json::json!({
                    "role": "tool",
                    "tool_call_id": m.tool_call_id.clone().unwrap_or_default(),
                    "content": m.content,
                });
            }

            if !m.tool_calls.is_empty() {
                return serde_json::json!({
                    "role": "assistant",
                    "content": if m.content.is_empty() {
                        serde_json::Value::Null
                    } else {
                        serde_json::json!(m.content)
                    },
                    "tool_calls": m.tool_calls.iter().map(|c| serde_json::json!({
                        "id": c.id,
                        "type": "function",
                        "function": { "name": wire_name(&c.name), "arguments": c.args },
                    })).collect::<Vec<_>>(),
                });
            }

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
    tools: &[ToolSpec],
    on_delta: DeltaSink<'_>,
) -> Result<StreamDone, CallError> {
    let lc = prov.compatible.to_lowercase();

    match lc.as_str() {
        "openai" => {
            openai_compat::stream(http, &prov.base_url, tok, remote_id, msgs, tools, on_delta).await
        }
        "anthropic" => {
            anthropic::stream(http, &prov.base_url, tok, remote_id, msgs, tools, on_delta).await
        }
        _ => Err(CallError {
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
    tools: &[ToolSpec],
) -> Result<(WireResp, String), CallError> {
    let lc = prov.compatible.to_lowercase();

    match lc.as_str() {
        "openai" => openai_compat::chat(http, &prov.base_url, tok, remote_id, msgs, tools).await,
        "anthropic" => anthropic::chat(http, &prov.base_url, tok, remote_id, msgs, tools).await,
        _ => Err(CallError {
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
