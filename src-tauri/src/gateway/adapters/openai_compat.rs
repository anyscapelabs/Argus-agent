use futures_util::StreamExt;
use reqwest::Client;

use super::{retry_after_secs, sse_events, CallErr, DeltaSink, WireResp};
use crate::gateway::schema::{StreamDone, WireMsg};

pub async fn stream(
    http: &Client,
    base_url: &str,
    tok: Option<String>,
    remote_id: &str,
    msgs: &[WireMsg],
    on_delta: DeltaSink<'_>,
) -> Result<StreamDone, CallErr> {
    let url = format!("{base_url}/chat/completions");
    let pl = serde_json::json!({ "model": remote_id, "messages": msgs, "stream": true });

    let mut req = http.post(&url).json(&pl);
    if let Some(t) = tok {
        req = req.bearer_auth(t);
    }

    let resp = req
        .send()
        .await
        .map_err(|e| CallErr {
            status: None,
            msg: e.to_string(),
            retry_after: None,
        })?;

    let status = resp.status().as_u16();
    if status != 200 {
        let ra = retry_after_secs(&resp);
        let body = resp.text().await.unwrap_or_default();
        return Err(CallErr {
            status: Some(status),
            msg: body,
            retry_after: ra,
        });
    }

    let mut buf = String::new();
    let mut done = StreamDone::default();
    let mut stream = resp.bytes_stream();

    while let Some(chunk) = stream.next().await {
        let bytes = chunk.map_err(|e| CallErr {
            status: None,
            msg: e.to_string(),
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

                if let Some(c) = v["choices"][0]["delta"]["content"].as_str() {
                    if !c.is_empty() {
                        on_delta(c).map_err(|e| CallErr {
                            status: None,
                            msg: e,
                            retry_after: None,
                        })?;
                        done.text.push_str(c);
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

    Ok(done)
}

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
    let resp = req.send().await.map_err(|e| CallErr {
        status: None,
        msg: e.to_string(),
        retry_after: None,
    })?;

    let status = resp.status().as_u16();
    let ra = retry_after_secs(&resp);
    let body = resp.text().await.map_err(|e| CallErr {
        status: Some(status),
        msg: e.to_string(),
        retry_after: None,
    })?;

    if status != 200 {
        return Err(CallErr {
            status: Some(status),
            msg: body,
            retry_after: ra,
        });
    }

    let wire: WireResp = serde_json::from_str(&body).map_err(|e| CallErr {
        status: Some(status),
        msg: e.to_string(),
        retry_after: None,
    })?;

    Ok((wire, body))
}
