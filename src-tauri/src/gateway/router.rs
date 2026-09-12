use std::time::{Duration, Instant};

use tauri::ipc::Channel;

use super::adapters;
use super::schema::{Avail, ChatReq, ChatResp, Provider, ReqLog, StreamEvent, WireUsage};
use super::{store, Gateway};

const MAX_ATTEMPTS: i64 = 10;

const FAST_MS: [u64; 9] = [
    1_000, 2_000, 4_000, 8_000, 15_000, 30_000, 60_000, 90_000, 120_000,
];
const RATE_MS: [u64; 9] = [
    15_000, 30_000, 60_000, 90_000, 120_000, 120_000, 120_000, 120_000, 120_000,
];

pub fn rank(provs: &[Provider], avails: &[Avail], mode: &str, pinned: &str) -> Vec<Avail> {
    let mut out = avails.to_vec();

    let key = |a: &Avail| -> (i64, i64) {
        let p = match provs.iter().find(|p| p.id == a.provider_id) {
            Some(p) => p,
            None => return (9_999, 9_999),
        };

        let mut r = 0i64;

        if mode == "prefer_free" && !p.free {
            r += 1_000;
        }

        if mode == "prefer_paid" && p.free {
            r += 1_000;
        }

        if mode == "pinned" && p.id != pinned {
            r += 1_000;
        }

        (r + p.priority, p.priority)
    };

    out.sort_by_key(&key);
    out
}

struct Resolved {
    provs: Vec<Provider>,
    av: Avail,
    req_json: Option<String>,
}

pub struct StreamStats {
    pub text: String,
    pub model_id: String,
    pub provider_id: String,
    pub tok_in: i64,
    pub tok_out: i64,
}

fn key_for(prov: &Provider) -> Result<Option<String>, String> {
    match store::secret_get(&prov.id) {
        Ok(t) => {
            if t.is_none() && !adapters::is_local(&prov.base_url) {
                return Err(format!(
                    "no API key stored for {} — reconnect it in Providers settings",
                    prov.name
                ));
            }

            Ok(t)
        }
        Err(_) => Err(format!(
            "could not read the stored key for {} — reconnect it in Providers settings",
            prov.name
        )),
    }
}

fn resolve(gw: &Gateway, req: &ChatReq) -> Result<Resolved, String> {
    let (mode, pinned, provs, avails) = {
        let conn = gw.conn.lock().map_err(|err| err.to_string())?;
        let mode = store::kv_get(&conn, "routing_mode").unwrap_or_else(|| "prefer_free".into());
        let pinned = store::kv_get(&conn, "pinned_provider").unwrap_or_default();
        let provs = store::list_providers(&conn)?;
        let avails = store::list_avail(&conn, &req.model)?;
        (mode, pinned, provs, avails)
    };

    if avails.is_empty() {
        return Err(format!("no connected provider serves model {}", req.model));
    }

    let ranked = rank(&provs, &avails, &mode, &pinned);
    let av = ranked
        .first()
        .ok_or("no provider serves this model")?
        .clone();

    Ok(Resolved {
        provs,
        av,
        req_json: serde_json::to_string(req).ok(),
    })
}

fn backoff_ms(status: Option<u16>, attempt: i64, retry_after: Option<u64>) -> u64 {
    if let Some(secs) = retry_after {
        return secs.saturating_mul(1000).min(120_000);
    }

    let ladder = if status == Some(429) {
        RATE_MS
    } else {
        FAST_MS
    };
    let base = ladder[(attempt - 1).clamp(0, 8) as usize];
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos() as u64)
        .unwrap_or(0);

    base + nanos % base / 4
}

pub async fn run(gw: &Gateway, req: &ChatReq) -> Result<ChatResp, String> {
    run_opts(gw, req, MAX_ATTEMPTS).await
}

pub async fn run_opts(gw: &Gateway, req: &ChatReq, max_attempts: i64) -> Result<ChatResp, String> {
    let Resolved {
        provs,
        av,
        req_json,
    } = resolve(gw, req)?;

    let prov = match provs.iter().find(|p| p.id == av.provider_id) {
        Some(p) => p,
        None => return Err("provider gone".into()),
    };

    let tok = key_for(prov)?;

    let mut attempt = 0i64;
    let mut rate_body: Option<String> = None;
    let mut same_429 = 0u32;

    loop {
        attempt += 1;

        let t0 = Instant::now();
        let res =
            adapters::dispatch(&gw.http, prov, &av.remote_model_id, tok.clone(), &req.msgs).await;
        let latency = t0.elapsed().as_millis() as i64;

        match res {
            Ok((wire, raw)) => {
                let usage = wire.usage.clone().unwrap_or(WireUsage {
                    prompt_tokens: 0,
                    completion_tokens: 0,
                });

                let cost = usage.prompt_tokens as f64 / 1000.0 * av.cost_in
                    + usage.completion_tokens as f64 / 1000.0 * av.cost_out;

                let content = wire
                    .choices
                    .first()
                    .map(|c| c.message.content.clone().unwrap_or_default())
                    .unwrap_or_default();

                let log = ReqLog {
                    model_id: Some(req.model.clone()),
                    provider_id: Some(prov.id.clone()),
                    remote_model_id: Some(av.remote_model_id.clone()),
                    attempt,
                    status: "ok".into(),
                    latency_ms: Some(latency),
                    tok_in: Some(usage.prompt_tokens as i64),
                    tok_out: Some(usage.completion_tokens as i64),
                    cost: Some(cost),
                    err_msg: None,
                    req_json: req_json.clone(),
                    resp_json: Some(raw),
                    prefix_hash: req.prefix_hash.clone(),
                };

                {
                    let conn = gw.conn.lock().map_err(|err| err.to_string())?;
                    store::log_req(&conn, &log)?;
                }

                return Ok(ChatResp {
                    content,
                    model_id: req.model.clone(),
                    provider_id: prov.id.clone(),
                    attempt,
                    latency_ms: latency,
                    tok_in: usage.prompt_tokens as i64,
                    tok_out: usage.completion_tokens as i64,
                    cost,
                });
            }

            Err(err) => {
                let log = ReqLog {
                    model_id: Some(req.model.clone()),
                    provider_id: Some(prov.id.clone()),
                    remote_model_id: Some(av.remote_model_id.clone()),
                    attempt,
                    status: "err".into(),
                    latency_ms: Some(latency),
                    tok_in: None,
                    tok_out: None,
                    cost: None,
                    err_msg: Some(err.msg.clone()),
                    req_json: req_json.clone(),
                    resp_json: None,
                    prefix_hash: req.prefix_hash.clone(),
                };

                {
                    let conn = gw.conn.lock().map_err(|err| err.to_string())?;
                    let _ = store::log_req(&conn, &log);
                }

                let msg = if attempt >= max_attempts {
                    format!("gave up after {attempt} attempts: {}", err.msg)
                } else {
                    err.msg.clone()
                };

                if !err.retryable() || attempt >= max_attempts {
                    return Err(msg);
                }

                if err.status == Some(429) {
                    same_429 = if rate_body.as_deref() == Some(err.msg.as_str()) {
                        same_429 + 1
                    } else {
                        1
                    };
                    rate_body = Some(err.msg.clone());

                    if same_429 >= 3 {
                        return Err(
                            "provider keeps returning the same rate limit — likely out of quota; try another model".into(),
                        );
                    }
                }

                tokio::time::sleep(Duration::from_millis(backoff_ms(
                    err.status,
                    attempt,
                    err.retry_after,
                )))
                .await;
            }
        }
    }
}

pub async fn stream_run(
    gw: &Gateway,
    req: &ChatReq,
    chan: &Channel<StreamEvent>,
) -> Result<StreamStats, String> {
    let Resolved {
        provs,
        av,
        req_json,
    } = resolve(gw, req)?;

    let prov = match provs.iter().find(|p| p.id == av.provider_id) {
        Some(p) => p,
        None => return Err("provider gone".into()),
    };

    let tok = key_for(prov)?;

    let mut attempt = 0i64;
    let mut rate_body: Option<String> = None;
    let mut same_429 = 0u32;

    loop {
        attempt += 1;
        let _ = chan.send(StreamEvent::Status {
            provider_id: prov.id.clone(),
            attempt,
        });

        let t0 = Instant::now();
        let mut chan_err: Option<String> = None;
        let mut sent_delta = false;

        let mut sink = |c: &str| -> Result<(), String> {
            if chan_err.is_some() {
                return Ok(());
            }

            match chan.send(StreamEvent::Delta { text: c.into() }) {
                Ok(()) => {
                    sent_delta = true;
                    Ok(())
                }
                Err(err) => {
                    chan_err = Some(err.to_string());
                    Ok(())
                }
            }
        };

        let res = adapters::dispatch_stream(
            &gw.http,
            prov,
            &av.remote_model_id,
            tok.clone(),
            &req.msgs,
            &mut sink,
        )
        .await;

        let latency = t0.elapsed().as_millis() as i64;

        match res {
            Ok(done) => {
                let tok_in = done.tok_in.unwrap_or(0) as i64;
                let tok_out = done.tok_out.unwrap_or(0) as i64;
                let cost =
                    tok_in as f64 / 1000.0 * av.cost_in + tok_out as f64 / 1000.0 * av.cost_out;

                let log = ReqLog {
                    model_id: Some(req.model.clone()),
                    provider_id: Some(prov.id.clone()),
                    remote_model_id: Some(av.remote_model_id.clone()),
                    attempt,
                    status: "ok".into(),
                    latency_ms: Some(latency),
                    tok_in: Some(tok_in),
                    tok_out: Some(tok_out),
                    cost: Some(cost),
                    err_msg: None,
                    req_json: req_json.clone(),
                    resp_json: Some(done.text.clone()),
                    prefix_hash: req.prefix_hash.clone(),
                };

                {
                    let conn = gw.conn.lock().map_err(|err| err.to_string())?;
                    store::log_req(&conn, &log)?;
                }

                chan.send(StreamEvent::Done {
                    model_id: req.model.clone(),
                    provider_id: prov.id.clone(),
                    attempt,
                    latency_ms: latency,
                    tok_in,
                    tok_out,
                    cost,
                })
                .map_err(|err| err.to_string())?;

                return Ok(StreamStats {
                    text: done.text,
                    model_id: req.model.clone(),
                    provider_id: prov.id.clone(),
                    tok_in,
                    tok_out,
                });
            }

            Err(err) => {
                let log = ReqLog {
                    model_id: Some(req.model.clone()),
                    provider_id: Some(prov.id.clone()),
                    remote_model_id: Some(av.remote_model_id.clone()),
                    attempt,
                    status: "err".into(),
                    latency_ms: Some(latency),
                    tok_in: None,
                    tok_out: None,
                    cost: None,
                    err_msg: Some(err.msg.clone()),
                    req_json: req_json.clone(),
                    resp_json: None,
                    prefix_hash: req.prefix_hash.clone(),
                };

                {
                    let conn = gw.conn.lock().map_err(|err| err.to_string())?;
                    let _ = store::log_req(&conn, &log);
                }

                if !err.retryable() || attempt >= MAX_ATTEMPTS {
                    let msg = if attempt >= MAX_ATTEMPTS {
                        format!("all {attempt} attempts failed: {}", err.msg)
                    } else {
                        err.msg.clone()
                    };
                    let _ = chan.send(StreamEvent::Err { msg: msg.clone() });
                    return Err(msg);
                }

                if err.status == Some(429) {
                    same_429 = if rate_body.as_deref() == Some(err.msg.as_str()) {
                        same_429 + 1
                    } else {
                        1
                    };
                    rate_body = Some(err.msg.clone());

                    if same_429 >= 3 {
                        let msg =
                            "provider keeps returning the same rate limit — likely out of quota; try another model"
                                .to_string();
                        let _ = chan.send(StreamEvent::Err { msg: msg.clone() });
                        return Err(msg);
                    }
                }

                if sent_delta {
                    let _ = chan.send(StreamEvent::Reset);
                }

                tokio::time::sleep(Duration::from_millis(backoff_ms(
                    err.status,
                    attempt,
                    err.retry_after,
                )))
                .await;
            }
        }
    }
}
