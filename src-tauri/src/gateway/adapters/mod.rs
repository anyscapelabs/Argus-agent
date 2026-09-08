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

// 401/403 reject the key; any other outcome (offline, missing endpoint) lets it through.
pub async fn verify_key(http: &Client, prov: &Provider, key: &str) -> Result<(), String> {
  let url = if prov.compatible == "Anthropic" {
    format!("{}/v1/models", prov.base_url.trim_end_matches('/'))
  } else {
    format!("{}/models", prov.base_url.trim_end_matches('/'))
  };
  let mut req = http.get(&url);
  if prov.compatible == "Anthropic" {
    req = req.header("x-api-key", key).header("anthropic-version", "2023-06-01");
  } else {
    req = req.header("Authorization", format!("Bearer {key}"));
  }
  match req.send().await {
    Ok(r) if r.status() == reqwest::StatusCode::UNAUTHORIZED || r.status() == reqwest::StatusCode::FORBIDDEN => {
      Err(format!("{} rejected this API key", prov.name))
    }
    _ => Ok(()),
  }
}
