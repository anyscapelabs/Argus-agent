use futures_util::StreamExt;
use reqwest::Client;

use super::{anthropic_content, retry_after_secs, sse_events, CallError, DeltaSink, WireResp};
use crate::gateway::schema::{StreamDone, ToolCall, ToolSpec, WireMsg};

fn payload(remote_id: &str, msgs: &[WireMsg], tools: &[ToolSpec], streaming: bool) -> serde_json::Value {
    let mut sys = String::new();
    let mut turns: Vec<serde_json::Value> = vec![];
    let mut pending: Vec<serde_json::Value> = vec![];

    let flush = |turns: &mut Vec<serde_json::Value>, pending: &mut Vec<serde_json::Value>| {
        if pending.is_empty() {
            return;
        }

        turns.push(serde_json::json!({ "role": "user", "content": std::mem::take(pending) }));
    };

    for m in msgs {
        if m.role == "system" {
            sys.push_str(&m.content);
            sys.push('\n');
            continue;
        }

        if m.role == "tool" {
            pending.push(serde_json::json!({
                "type": "tool_result",
                "tool_use_id": m.tool_call_id.clone().unwrap_or_default(),
                "content": m.content,
            }));
            continue;
        }

        flush(&mut turns, &mut pending);

        if m.role == "assistant" && !m.tool_calls.is_empty() {
            let mut parts: Vec<serde_json::Value> = vec![];

            if !m.content.trim().is_empty() {
                parts.push(serde_json::json!({ "type": "text", "text": m.content }));
            }

            for c in &m.tool_calls {
                let input: serde_json::Value =
                    serde_json::from_str(c.args.trim()).unwrap_or(serde_json::json!({}));

                parts.push(serde_json::json!({
                    "type": "tool_use",
                    "id": c.id,
                    "name": c.name,
                    "input": input,
                }));
            }

            turns.push(serde_json::json!({ "role": "assistant", "content": parts }));
            continue;
        }

        turns.push(serde_json::json!({ "role": m.role, "content": anthropic_content(m) }));
    }

    flush(&mut turns, &mut pending);

    let mut pl = if streaming {
        serde_json::json!({ "model": remote_id, "max_tokens": 8192, "messages": turns, "stream": true })
    } else {
        serde_json::json!({ "model": remote_id, "max_tokens": 8192, "messages": turns })
    };

    if !tools.is_empty() {
        pl["tools"] = serde_json::json!(tools
            .iter()
            .map(|t| serde_json::json!({
                "name": t.name,
                "description": t.description,
                "input_schema": t.parameters,
            }))
            .collect::<Vec<_>>());
    }

    if !sys.is_empty() {
        pl["system"] = serde_json::json!([
            { "type": "text", "text": sys.trim(), "cache_control": { "type": "ephemeral" } }
        ]);
    }

    let n = turns.len();
    for i in n.saturating_sub(3)..n {
        if let Some(text) = pl["messages"][i]["content"].as_str() {
            pl["messages"][i]["content"] = serde_json::json!([
                { "type": "text", "text": text, "cache_control": { "type": "ephemeral" } }
            ]);
            continue;
        }

        if let Some(arr) = pl["messages"][i]["content"].as_array_mut() {
            for blk in arr.iter_mut().rev() {
                let is_text = blk.get("type").and_then(|t| t.as_str()).is_some_and(|t| {
                    t == "text" || t == "tool_use" || t == "tool_result"
                });
                if is_text {
                    blk["cache_control"] = serde_json::json!({ "type": "ephemeral" });
                    break;
                }
            }
        }
    }

    pl
}

pub async fn stream(
    http: &Client,
    base_url: &str,
    tok: Option<String>,
    remote_id: &str,
    msgs: &[WireMsg],
    tools: &[ToolSpec],
    on_delta: DeltaSink<'_>,
) -> Result<StreamDone, CallError> {
    let pl = payload(remote_id, msgs, tools, true);
    let url = format!("{base_url}/v1/messages");

    let mut req = http
        .post(&url)
        .header("anthropic-version", "2023-06-01")
        .header("accept", "text/event-stream")
        .json(&pl);

    if let Some(t) = tok {
        req = req.header("x-api-key", t);
    }

    let resp = req.send().await.map_err(|err| CallError {
        status: None,
        msg: err.to_string(),
        retry_after: None,
    })?;

    let status = resp.status().as_u16();
    if status != 200 {
        let ra = retry_after_secs(&resp);
        let body = resp.text().await.unwrap_or_default();
        return Err(CallError {
            status: Some(status),
            msg: body,
            retry_after: ra,
        });
    }

    let mut buf = String::new();
    let mut done = StreamDone::default();
    let mut acc: Vec<(String, String, String)> = vec![];
    let mut stream = resp.bytes_stream();

    while let Some(chunk) = stream.next().await {
        let bytes = chunk.map_err(|err| CallError {
            status: None,
            msg: err.to_string(),
            retry_after: None,
        })?;

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
                    Some("content_block_start") => {
                        if v["content_block"]["type"] == "tool_use" {
                            acc.push((
                                v["content_block"]["id"]
                                    .as_str()
                                    .unwrap_or_default()
                                    .to_string(),
                                v["content_block"]["name"]
                                    .as_str()
                                    .unwrap_or_default()
                                    .to_string(),
                                String::new(),
                            ));
                        }
                    }
                    Some("content_block_delta") => {
                        if v["delta"]["type"] == "input_json_delta" {
                            if let Some(frag) = v["delta"]["partial_json"].as_str() {
                                if let Some(last) = acc.last_mut() {
                                    last.2.push_str(frag);
                                }
                            }
                        } else if let Some(c) = v["delta"]["text"].as_str() {
                            if !c.is_empty() {
                                on_delta(c).map_err(|err| CallError {
                                    status: None,
                                    msg: err,
                                    retry_after: None,
                                })?;
                                done.text.push_str(c);
                            }
                        }
                    }
                    Some("message_start") => {
                        done.tok_in = v["message"]["usage"]["input_tokens"]
                            .as_u64()
                            .or(done.tok_in);
                    }
                    Some("message_delta") => {
                        done.tok_out = v["usage"]["output_tokens"].as_u64().or(done.tok_out);

                        if v["delta"]["stop_reason"] == "max_tokens" {
                            done.truncated = true;
                        }
                    }
                    Some("error") => {
                        return Err(CallError {
                            status: None,
                            msg: v["error"]
                                .get("message")
                                .and_then(|m| m.as_str())
                                .unwrap_or("stream error")
                                .to_string(),
                            retry_after: None,
                        });
                    }
                    _ => {}
                }
            }
        }
    }

    done.tool_calls = acc
        .into_iter()
        .filter(|(_, name, _)| !name.is_empty())
        .enumerate()
        .map(|(i, (id, name, args))| ToolCall {
            id: if id.is_empty() { format!("call_{i}") } else { id },
            name,
            args: if args.trim().is_empty() {
                "{}".into()
            } else {
                args
            },
        })
        .collect();

    Ok(done)
}

pub async fn chat(
    http: &Client,
    base_url: &str,
    tok: Option<String>,
    remote_id: &str,
    msgs: &[WireMsg],
    tools: &[ToolSpec],
) -> Result<(WireResp, String), CallError> {
    let pl = payload(remote_id, msgs, tools, false);
    let url = format!("{base_url}/v1/messages");

    let mut req = http
        .post(&url)
        .header("anthropic-version", "2023-06-01")
        .json(&pl);
    if let Some(t) = tok {
        req = req.header("x-api-key", t);
    }

    let resp = req.send().await.map_err(|err| CallError {
        status: None,
        msg: err.to_string(),
        retry_after: None,
    })?;

    let status = resp.status().as_u16();
    let ra = retry_after_secs(&resp);
    let body = resp.text().await.map_err(|err| CallError {
        status: Some(status),
        msg: err.to_string(),
        retry_after: None,
    })?;

    if status != 200 {
        return Err(CallError {
            status: Some(status),
            msg: body,
            retry_after: ra,
        });
    }

    let v: serde_json::Value = serde_json::from_str(&body).map_err(|err| CallError {
        status: Some(status),
        msg: err.to_string(),
        retry_after: None,
    })?;

    let text = v["content"]
        .as_array()
        .map(|blks| {
            blks.iter()
                .filter_map(|b| b["text"].as_str())
                .collect::<Vec<_>>()
                .join("")
        })
        .unwrap_or_default();

    let wire = serde_json::json!({
        "choices": [{ "message": { "role": "assistant", "content": text } }],
        "usage": {
            "prompt_tokens": v["usage"]["input_tokens"].as_u64().unwrap_or(0),
            "completion_tokens": v["usage"]["output_tokens"].as_u64().unwrap_or(0)
        }
    });

    let wire: WireResp = serde_json::from_value(wire).map_err(|err| CallError {
        status: Some(status),
        msg: err.to_string(),
        retry_after: None,
    })?;

    Ok((wire, body))
}
