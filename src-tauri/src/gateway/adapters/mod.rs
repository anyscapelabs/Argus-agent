pub mod anthropic;
pub mod openai_compat;

use reqwest::Client;

use super::schema::{Provider, WireMsg, WireResp};

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
