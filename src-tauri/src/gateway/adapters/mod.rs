pub mod anthropic;
pub mod openai_compat;

use reqwest::Client;

use super::schema::{Provider, StreamDone, WireMsg, WireResp};

#[derive(Debug)]
pub struct CallErr {
  pub status: Option<u16>,
  pub msg: String,
}

impl CallErr {
  pub fn retryable(&self) -> bool {
    match self.status {
      None => true, // transport dead, make a plan elsewhere
      Some(s) => s == 402 || s == 408 || s == 429 || s >= 500,
    }
  }
}

// Split complete SSE events off the buffer, keep the tail for the next chunk.
pub fn sse_events(buf: &mut String) -> Vec<String> {
  let mut out = vec![];
  while let Some(pos) = buf.find("\n\n") {
    out.push(buf.drain(..pos + 2).collect());
  }
  out
}

pub type DeltaSink<'a> = &'a mut (dyn FnMut(&str) -> Result<(), String> + Send + 'a);

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
    "openai" => openai_compat::stream(http, &prov.base_url, tok, remote_id, msgs, on_delta).await,
    "anthropic" => anthropic::stream(http, &prov.base_url, tok, remote_id, msgs, on_delta).await,
    _ => Err(CallErr {
      status: None,
      msg: format!("unknown compatible dialect {}", prov.compatible),
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
    }),
  }
}

// Keyless providers (ollama-style) are local only; cloud endpoints need a key.
pub fn is_local(base_url: &str) -> bool {
  let host = base_url
    .trim_start_matches("http://")
    .trim_start_matches("https://");
  host.starts_with("localhost") || host.starts_with("127.0.0.1") || host.starts_with("[::1]")
}

// /models is public on some providers (OpenRouter), so a GET there accepts bad
// keys. An empty chat POST is the cheapest request that servers actually gate:
// 401/403 with a bad key, 4xx validation with a good one.
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
    req = req.header("x-api-key", key).header("anthropic-version", "2023-06-01");
  } else {
    req = req.bearer_auth(key);
  }
  match req.send().await {
    Ok(r) if r.status() == reqwest::StatusCode::UNAUTHORIZED || r.status() == reqwest::StatusCode::FORBIDDEN => {
      Err(format!("{} rejected this API key", prov.name))
    }
    _ => Ok(()), // offline or validation error: auth was not the blocker
  }
}
