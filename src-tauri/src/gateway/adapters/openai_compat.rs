use reqwest::Client;

use super::{CallErr, WireResp};
use crate::gateway::schema::WireMsg;

pub async fn chat(
  http: &Client,
  base_url: &str,
  tok: Option<String>,
  remote_id: &str,
  msgs: &[WireMsg],
) -> Result<(WireResp, String), CallErr> {
  let url = format!("{base_url}/chat/completions");
  let pl = serde_json::json!({ "model": remote_id, "messages": msgs, "stream": false });
  let mut req = http.post(&url).json(&pl);
  if let Some(t) = tok {
    req = req.bearer_auth(t);
  }
  send(req).await
}

pub async fn send(req: reqwest::RequestBuilder) -> Result<(WireResp, String), CallErr> {
  let resp = req.send().await.map_err(|e| CallErr { status: None, msg: e.to_string() })?;
  let status = resp.status().as_u16();
  let body = resp.text().await.map_err(|e| CallErr { status: Some(status), msg: e.to_string() })?;
  if status != 200 {
    return Err(CallErr { status: Some(status), msg: body }); // Drop it
  }
  let wire: WireResp = serde_json::from_str(&body).map_err(|e| CallErr { status: Some(status), msg: e.to_string() })?;
  Ok((wire, body))
}
