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
  let mut sys = String::new();
  let mut turns: Vec<serde_json::Value> = vec![];
  for m in msgs {
    if m.role == "system" {
      sys.push_str(&m.content);
      sys.push('\n');
    } else {
      turns.push(serde_json::json!({ "role": m.role, "content": m.content }));
    }
  }
  let mut pl = serde_json::json!({ "model": remote_id, "max_tokens": 4096, "messages": turns });
  if !sys.is_empty() {
    pl["system"] = serde_json::Value::String(sys.trim().into());
  }

  let url = format!("{base_url}/v1/messages");
  let mut req = http.post(&url).header("anthropic-version", "2023-06-01").json(&pl);
  if let Some(t) = tok {
    req = req.header("x-api-key", t);
  }

  let resp = req.send().await.map_err(|e| CallErr { status: None, msg: e.to_string() })?;
  let status = resp.status().as_u16();
  let body = resp.text().await.map_err(|e| CallErr { status: Some(status), msg: e.to_string() })?;
  if status != 200 {
    return Err(CallErr { status: Some(status), msg: body }); // Drop it
  }

  let v: serde_json::Value = serde_json::from_str(&body).map_err(|e| CallErr { status: Some(status), msg: e.to_string() })?;
  let text = v["content"]
    .as_array()
    .map(|blks| blks.iter().filter_map(|b| b["text"].as_str()).collect::<Vec<_>>().join(""))
    .unwrap_or_default();
  let wire = serde_json::json!({
    "choices": [{ "message": { "role": "assistant", "content": text } }],
    "usage": {
      "prompt_tokens": v["usage"]["input_tokens"].as_u64().unwrap_or(0),
      "completion_tokens": v["usage"]["output_tokens"].as_u64().unwrap_or(0)
    }
  });
  let wire: WireResp = serde_json::from_value(wire).map_err(|e| CallErr { status: Some(status), msg: e.to_string() })?;
  Ok((wire, body))
}
