use futures_util::StreamExt;
use reqwest::Client;

use super::{openai_msgs, retry_after_secs, sse_events, CallError, DeltaSink, WireResp};
use crate::gateway::schema::{StreamDone, ToolCall, ToolSpec, WireMsg};

struct CallAcc {
    id: String,
    name: String,
    args: String,
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
    let url = format!("{base_url}/chat/completions");
    let mut pl = serde_json::json!({
        "model": remote_id,
        "messages": openai_msgs(msgs),
        "stream": true,
        "stream_options": { "include_usage": true },
    });

    if !tools.is_empty() {
        pl["tools"] = serde_json::json!(tools
            .iter()
            .map(|t| serde_json::json!({
                "type": "function",
                "function": {
                    "name": t.name,
                    "description": t.description,
                    "parameters": t.parameters,
                },
            }))
            .collect::<Vec<_>>());
    }

    let mut req = http.post(&url).json(&pl);
    if let Some(t) = tok {
        req = req.bearer_auth(t);
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
    let mut acc: Vec<CallAcc> = vec![];
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
                    Some("[DONE]") | None => continue,
                    Some(d) => d,
                };

                let v: serde_json::Value = match serde_json::from_str(data) {
                    Ok(v) => v,
                    Err(_) => continue,
                };

                if v.get("error").is_some() {
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

                if let Some(c) = v["choices"][0]["delta"]["content"].as_str() {
                    if !c.is_empty() {
                        on_delta(c).map_err(|err| CallError {
                            status: None,
                            msg: err,
                            retry_after: None,
                        })?;
                        done.text.push_str(c);
                    }
                }

                if v["choices"][0]["finish_reason"] == "length" {
                    done.truncated = true;
                }

                if let Some(calls) = v["choices"][0]["delta"]["tool_calls"].as_array() {
                    for c in calls {
                        let idx = c["index"].as_u64().unwrap_or(0) as usize;

                        while acc.len() <= idx {
                            acc.push(CallAcc {
                                id: String::new(),
                                name: String::new(),
                                args: String::new(),
                            });
                        }

                        let slot = &mut acc[idx];

                        if let Some(id) = c["id"].as_str() {
                            slot.id = id.to_string();
                        }

                        if let Some(name) = c["function"]["name"].as_str() {
                            slot.name = name.to_string();
                        }

                        if let Some(frag) = c["function"]["arguments"].as_str() {
                            slot.args.push_str(frag);
                        }
                    }
                }

                if let Some(t) = v["usage"]["prompt_tokens"].as_u64() {
                    done.tok_in = Some(t);
                }

                if let Some(t) = v["usage"]["completion_tokens"].as_u64() {
                    done.tok_out = Some(t);
                }
            }
        }
    }

    done.tool_calls = acc
        .into_iter()
        .filter(|c| !c.name.is_empty())
        .enumerate()
        .map(|(i, c)| ToolCall {
            id: if c.id.is_empty() {
                format!("call_{i}")
            } else {
                c.id
            },
            name: c.name,
            args: c.args,
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
    let url = format!("{base_url}/chat/completions");
    let mut pl =
        serde_json::json!({ "model": remote_id, "messages": openai_msgs(msgs), "stream": false });

    if !tools.is_empty() {
        pl["tools"] = serde_json::json!(tools
            .iter()
            .map(|t| serde_json::json!({
                "type": "function",
                "function": {
                    "name": t.name,
                    "description": t.description,
                    "parameters": t.parameters,
                },
            }))
            .collect::<Vec<_>>());
    }

    let mut req = http.post(&url).json(&pl);
    if let Some(t) = tok {
        req = req.bearer_auth(t);
    }

    send(req).await
}

pub async fn send(req: reqwest::RequestBuilder) -> Result<(WireResp, String), CallError> {
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

    let wire: WireResp = serde_json::from_str(&body).map_err(|err| CallError {
        status: Some(status),
        msg: err.to_string(),
        retry_after: None,
    })?;

    Ok((wire, body))
}
