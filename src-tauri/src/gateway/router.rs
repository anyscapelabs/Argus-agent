use std::time::Instant;

use tauri::ipc::Channel;

use super::adapters;
use super::schema::{Avail, ChatReq, ChatResp, Provider, ReqLog, StreamEvent, WireUsage};
use super::{store, Gateway};

// Rank providers: user routing mode first, then provider priority.
pub fn rank(provs: &[Provider], avails: &[Avail], mode: &str, pinned: &str) -> Vec<Avail> {
  let mut out = avails.to_vec();
  let key = |a: &Avail| -> (i64, i64) {
    let p = match provs.iter().find(|p| p.id == a.provider_id) {
      Some(p) => p,
      None => return (9_999, 9_999), // Drop it, provider gone
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
  ranked: Vec<Avail>,
  provs: Vec<Provider>,
  req_json: Option<String>,
}

fn resolve(gw: &Gateway, req: &ChatReq) -> Result<Resolved, String> {
  let (mode, pinned, provs, avails) = {
    let conn = gw.conn.lock().map_err(|e| e.to_string())?;
    let mode = store::kv_get(&conn, "routing_mode").unwrap_or_else(|| "prefer_free".into());
    let pinned = store::kv_get(&conn, "pinned_provider").unwrap_or_default();
    let provs = store::list_providers(&conn)?;
    let avails = store::list_avail(&conn, &req.model)?;
    (mode, pinned, provs, avails)
  };
  if avails.is_empty() {
    return Err(format!("no enabled provider serves model {}", req.model)); // Drop it
  }
  let ranked = rank(&provs, &avails, &mode, &pinned);
  Ok(Resolved { ranked, provs, req_json: serde_json::to_string(req).ok() })
}

pub async fn run(gw: &Gateway, req: &ChatReq) -> Result<ChatResp, String> {
  let Resolved { ranked, provs, req_json } = resolve(gw, req)?;

  let mut attempt = 0i64;
  let mut last_err = String::new();
  for av in &ranked {
    let prov = match provs.iter().find(|p| p.id == av.provider_id) {
      Some(p) => p,
      None => continue,
    };
    attempt += 1;
    let tok = store::secret_get(&prov.id).unwrap_or(None);
    let t0 = Instant::now();
    let res = adapters::dispatch(&gw.http, prov, &av.remote_model_id, tok, &req.msgs).await;
    let latency = t0.elapsed().as_millis() as i64;

    match res {
      Ok((wire, raw)) => {
        let usage = wire.usage.clone().unwrap_or(WireUsage { prompt_tokens: 0, completion_tokens: 0 });
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
        };
        {
          let conn = gw.conn.lock().map_err(|e| e.to_string())?;
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
        }); // Sorted
      }
      Err(e) => {
        last_err = e.msg.clone();
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
          err_msg: Some(e.msg.clone()),
          req_json: req_json.clone(),
          resp_json: None,
        };
        {
          let conn = gw.conn.lock().map_err(|e| e.to_string())?;
          let _ = store::log_req(&conn, &log); // keep fallback moving even if log fails
        }
        if !e.retryable() {
          return Err(e.msg); // Drop it, not a rate/quota issue
        }
      }
    }
  }
  Err(format!("all {attempt} attempts failed: {last_err}"))
}

pub async fn stream_run(
  gw: &Gateway,
  req: &ChatReq,
  chan: &Channel<StreamEvent>,
) -> Result<(), String> {
  let Resolved { ranked, provs, req_json } = resolve(gw, req)?;
  let mut attempt = 0i64;
  let mut last_err = String::new();

  for av in &ranked {
    let prov = match provs.iter().find(|p| p.id == av.provider_id) {
      Some(p) => p,
      None => continue,
    };
    attempt += 1;
    let _ = chan.send(StreamEvent::Status { provider_id: prov.id.clone(), attempt });

    let tok = store::secret_get(&prov.id).unwrap_or(None);
    let t0 = Instant::now();
    let mut chan_err: Option<String> = None;
    let mut sink = |c: &str| -> Result<(), String> {
      if chan_err.is_some() {
        return Ok(()); // channel dead, keep reading so the model finishes
      }
      match chan.send(StreamEvent::Delta { text: c.into() }) {
        Ok(()) => Ok(()),
        Err(e) => {
          chan_err = Some(e.to_string());
          Ok(())
        }
      }
    };
    let res = adapters::dispatch_stream(&gw.http, prov, &av.remote_model_id, tok, &req.msgs, &mut sink).await;
    let latency = t0.elapsed().as_millis() as i64;

    match res {
      Ok(done) => {
        let tok_in = done.tok_in.unwrap_or(0) as i64;
        let tok_out = done.tok_out.unwrap_or(0) as i64;
        let cost = tok_in as f64 / 1000.0 * av.cost_in + tok_out as f64 / 1000.0 * av.cost_out;
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
          resp_json: Some(done.text),
        };
        {
          let conn = gw.conn.lock().map_err(|e| e.to_string())?;
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
        .map_err(|e| e.to_string())?;
        return Ok(()); // Sorted
      }
      Err(e) => {
        last_err = e.msg.clone();
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
          err_msg: Some(e.msg.clone()),
          req_json: req_json.clone(),
          resp_json: None,
        };
        {
          let conn = gw.conn.lock().map_err(|e| e.to_string())?;
          let _ = store::log_req(&conn, &log); // keep fallback moving even if log fails
        }
        if !e.retryable() {
          let _ = chan.send(StreamEvent::Err { msg: e.msg.clone() });
          return Err(e.msg); // Drop it
        }
      }
    }
  }
  let msg = format!("all {attempt} attempts failed: {last_err}");
  let _ = chan.send(StreamEvent::Err { msg: msg.clone() });
  Err(msg)
}
