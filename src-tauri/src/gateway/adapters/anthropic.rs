use futures_util::StreamExt;
use reqwest::Client;

use super::{sse_events, CallErr, DeltaSink, WireResp};
use crate::gateway::schema::{StreamDone, WireMsg};

pub async fn stream(
  http: &Client,
  base_url: &str,
  tok: Option<String>,
  remote_id: &str,
  msgs: &[WireMsg],
  on_delta: DeltaSink<'_>,
) -> Result<StreamDone, CallErr> {
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
  let mut pl = serde_json::json!({ "model": remote_id, "max_tokens": 4096, "messages": turns, "stream": true });
  if !sys.is_empty() {
    pl["system"] = serde_json::Value::String(sys.trim().into());
  }

  let url = format!("{base_url}/v1/messages");
  let mut req = http
    .post(&url)
    .header("anthropic-version", "2023-06-01")
    .header("accept", "text/event-stream")
    .json(&pl);
  if let Some(t) = tok {
    req = req.header("x-api-key", t);
  }

  let resp = req.send().await.map_err(|e| CallErr { status: None, msg: e.to_string() })?;
  let status = resp.status().as_u16();
  if status != 200 {
    let body = resp.text().await.unwrap_or_default();
    return Err(CallErr { status: Some(status), msg: body }); // Drop it
  }

  let mut buf = String::new();
  let mut done = StreamDone::default();
  let mut stream = resp.bytes_stream();
  while let Some(chunk) = stream.next().await {
    let bytes = chunk.map_err(|e| CallErr { status: None, msg: e.to_string() })?;
    buf.push_str(&String::from_utf8_lossy(&bytes));
    for ev in sse_events(&mut buf) {
      for line in ev.lines() {
        let data = match line.strip_prefix("data:").map(str::trim) {
          Some(d) if !d.is_empty() => d,
          _ => continue,
        };
        let v: serde_json::Value = match serde_json::from_str(data) {
          Ok(v) => v,
          Err(_) => continue,
        };
        match v["type"].as_str() {
          Some("content_block_delta") => {
            if let Some(c) = v["delta"]["text"].as_str() {
              if !c.is_empty() {
                on_delta(c).map_err(|e| CallErr { status: None, msg: e })?;
                done.text.push_str(c);
              }
            }
          }
          Some("message_start") => {
            done.tok_in = v["message"]["usage"]["input_tokens"].as_u64().or(done.tok_in);
          }
          Some("message_delta") => {
            done.tok_out = v["usage"]["output_tokens"].as_u64().or(done.tok_out);
          }
          _ => {}
        }
      }
    }
  }
  Ok(done)
}

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
