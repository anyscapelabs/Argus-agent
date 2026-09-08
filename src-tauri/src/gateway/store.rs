use std::path::Path;

use keyring::Entry;
use rusqlite::{params, Connection, OptionalExtension};

use super::catalog;
use super::schema::{Avail, ModelEntry, Provider, ReqLog};

pub fn open(db_path: &Path) -> Result<Connection, String> {
  let conn = Connection::open(db_path).map_err(|e| e.to_string())?;
  conn.execute_batch(super::schema::MIGRATE).map_err(|e| e.to_string())?;
  seed_chk(&conn)?;
  Ok(conn)
}

fn seed_chk(conn: &Connection) -> Result<(), String> {
  if kv_get(conn, "seeded").is_some() {
    return Ok(()); // Sorted, already seeded
  }
  for p in catalog::providers() {
    upsert_provider(conn, &p)?;
  }
  for m in catalog::models() {
    add_model(conn, &m)?;
  }
  for a in catalog::avail() {
    link_model(conn, &a)?;
  }
  kv_set(conn, "seeded", "1")?;
  Ok(())
}

pub fn upsert_provider(conn: &Connection, p: &Provider) -> Result<(), String> {
  conn
    .execute(
      "INSERT INTO providers (id, compatible, base_url, api_key_ref, enabled, free, priority)
       VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
       ON CONFLICT(id) DO UPDATE SET compatible=?2, base_url=?3, api_key_ref=?4, enabled=?5, free=?6, priority=?7",
      params![p.id, p.compatible, p.base_url, p.api_key_ref, p.enabled as i64, p.free as i64, p.priority],
    )
    .map_err(|e| e.to_string())?;
  Ok(())
}

pub fn list_providers(conn: &Connection) -> Result<Vec<Provider>, String> {
  let mut stmt = conn
    .prepare("SELECT id, compatible, base_url, api_key_ref, enabled, free, priority FROM providers ORDER BY priority")
    .map_err(|e| e.to_string())?;
  let rows = stmt
    .query_map([], |r| {
      Ok(Provider {
        id: r.get(0)?,
        compatible: r.get(1)?,
        base_url: r.get(2)?,
        api_key_ref: r.get(3)?,
        enabled: r.get::<_, i64>(4)? != 0,
        free: r.get::<_, i64>(5)? != 0,
        priority: r.get(6)?,
      })
    })
    .map_err(|e| e.to_string())?;
  rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
}

pub fn add_model(conn: &Connection, m: &ModelEntry) -> Result<(), String> {
  conn
    .execute(
      "INSERT INTO models (id, display_name, family, capabilities, suggested_tier)
       VALUES (?1, ?2, ?3, ?4, ?5)
       ON CONFLICT(id) DO UPDATE SET display_name=?2, family=?3, capabilities=?4, suggested_tier=?5",
      params![m.id, m.display_name, m.family, m.capabilities, m.suggested_tier],
    )
    .map_err(|e| e.to_string())?;
  Ok(())
}

pub fn list_models(conn: &Connection) -> Result<Vec<ModelEntry>, String> {
  let mut stmt = conn
    .prepare("SELECT id, display_name, family, capabilities, suggested_tier FROM models ORDER BY family, id")
    .map_err(|e| e.to_string())?;
  let rows = stmt
    .query_map([], |r| {
      Ok(ModelEntry {
        id: r.get(0)?,
        display_name: r.get(1)?,
        family: r.get(2)?,
        capabilities: r.get(3)?,
        suggested_tier: r.get(4)?,
      })
    })
    .map_err(|e| e.to_string())?;
  rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
}

pub fn link_model(conn: &Connection, a: &Avail) -> Result<(), String> {
  conn
    .execute(
      "INSERT INTO model_providers (model_id, provider_id, remote_model_id, cost_in, cost_out)
       VALUES (?1, ?2, ?3, ?4, ?5)
       ON CONFLICT(model_id, provider_id) DO UPDATE SET remote_model_id=?3, cost_in=?4, cost_out=?5",
      params![a.model_id, a.provider_id, a.remote_model_id, a.cost_in, a.cost_out],
    )
    .map_err(|e| e.to_string())?;
  Ok(())
}

pub fn list_avail(conn: &Connection, model_id: &str) -> Result<Vec<Avail>, String> {
  let mut stmt = conn
    .prepare(
      "SELECT mp.model_id, mp.provider_id, mp.remote_model_id, mp.cost_in, mp.cost_out
       FROM model_providers mp
       JOIN providers p ON p.id = mp.provider_id
       WHERE mp.model_id = ?1 AND p.enabled = 1",
    )
    .map_err(|e| e.to_string())?;
  let rows = stmt
    .query_map(params![model_id], |r| {
      Ok(Avail {
        model_id: r.get(0)?,
        provider_id: r.get(1)?,
        remote_model_id: r.get(2)?,
        cost_in: r.get(3)?,
        cost_out: r.get(4)?,
      })
    })
    .map_err(|e| e.to_string())?;
  rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
}

pub fn log_req(conn: &Connection, l: &ReqLog) -> Result<(), String> {
  conn
    .execute(
      "INSERT INTO request_log (model_id, provider_id, remote_model_id, attempt, status, latency_ms, tok_in, tok_out, cost, err_msg, req_json, resp_json)
       VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
      params![l.model_id, l.provider_id, l.remote_model_id, l.attempt, l.status, l.latency_ms, l.tok_in, l.tok_out, l.cost, l.err_msg, l.req_json, l.resp_json],
    )
    .map_err(|e| e.to_string())?;
  Ok(())
}

pub fn list_logs(conn: &Connection, limit: i64) -> Result<Vec<ReqLog>, String> {
  let mut stmt = conn
    .prepare(
      "SELECT model_id, provider_id, remote_model_id, attempt, status, latency_ms, tok_in, tok_out, cost, err_msg, req_json, resp_json
       FROM request_log ORDER BY id DESC LIMIT ?1",
    )
    .map_err(|e| e.to_string())?;
  let rows = stmt
    .query_map(params![limit], |r| {
      Ok(ReqLog {
        model_id: r.get(0)?,
        provider_id: r.get(1)?,
        remote_model_id: r.get(2)?,
        attempt: r.get(3)?,
        status: r.get(4)?,
        latency_ms: r.get(5)?,
        tok_in: r.get(6)?,
        tok_out: r.get(7)?,
        cost: r.get(8)?,
        err_msg: r.get(9)?,
        req_json: r.get(10)?,
        resp_json: r.get(11)?,
      })
    })
    .map_err(|e| e.to_string())?;
  rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
}

pub fn kv_get(conn: &Connection, k: &str) -> Option<String> {
  conn
    .query_row("SELECT v FROM kv WHERE k = ?1", params![k], |r| r.get::<_, String>(0))
    .optional()
    .ok()
    .flatten()
}

pub fn kv_set(conn: &Connection, k: &str, v: &str) -> Result<(), String> {
  conn
    .execute("INSERT INTO kv (k, v) VALUES (?1, ?2) ON CONFLICT(k) DO UPDATE SET v=?2", params![k, v])
    .map_err(|e| e.to_string())?;
  Ok(())
}

pub fn secret_set(provider_id: &str, tok: &str) -> Result<(), String> {
  let entry = Entry::new("argus-gw", provider_id).map_err(|e| e.to_string())?;
  entry.set_password(tok).map_err(|e| e.to_string())
}

pub fn secret_get(provider_id: &str) -> Result<Option<String>, String> {
  let entry = Entry::new("argus-gw", provider_id).map_err(|e| e.to_string())?;
  match entry.get_password() {
    Ok(v) => Ok(Some(v)),
    Err(keyring::Error::NoEntry) => Ok(None),
    Err(e) => Err(e.to_string()),
  }
}

pub fn secret_del(provider_id: &str) -> Result<(), String> {
  let entry = Entry::new("argus-gw", provider_id).map_err(|e| e.to_string())?;
  match entry.delete_credential() {
    Ok(()) => Ok(()),
    Err(keyring::Error::NoEntry) => Ok(()),
    Err(e) => Err(e.to_string()),
  }
}
