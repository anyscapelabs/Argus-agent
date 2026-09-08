use std::path::Path;

use keyring::Entry;
use rusqlite::{params, Connection, OptionalExtension};

use super::catalog;
use super::schema::{Avail, ChatModel, ModelEntry, Provider, ProviderModel, ReqLog};

pub fn open(db_path: &Path) -> Result<Connection, String> {
  let conn = Connection::open(db_path).map_err(|e| e.to_string())?;
  conn.execute_batch(super::schema::MIGRATE).map_err(|e| e.to_string())?;
  ensure_cols(&conn)?;
  seed_chk(&conn)?;
  reconcile_connected(&conn)?;
  Ok(conn)
}

// The keyring decides connected: a stored key means connected, anything else
// reconnects through gw_connect. Repairs rows carried over from the enabled era.
fn reconcile_connected(conn: &Connection) -> Result<(), String> {
  let unnamed: Vec<String> = {
    let mut stmt = conn
      .prepare("SELECT id FROM providers WHERE name = ''")
      .map_err(|e| e.to_string())?;
    let map = stmt
      .query_map([], |r| r.get::<_, String>(0))
      .map_err(|e| e.to_string())?;
    map.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())?
  };
  for id in &unnamed {
    conn
      .execute("DELETE FROM model_providers WHERE provider_id = ?1", params![id])
      .map_err(|e| e.to_string())?;
    conn
      .execute("DELETE FROM providers WHERE id = ?1", params![id])
      .map_err(|e| e.to_string())?;
  }
  let rows: Vec<(String, bool)> = {
    let mut stmt = conn
      .prepare("SELECT id, connected FROM providers")
      .map_err(|e| e.to_string())?;
    let map = stmt
      .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? != 0)))
      .map_err(|e| e.to_string())?;
    map.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())?
  };
  for (id, connected) in rows {
    let has = secret_get(&id).unwrap_or(None).is_some();
    if has != connected {
      set_connected(conn, &id, has)?;
    }
  }
  Ok(())
}

// Dev DBs predate name/connected/logo_url/doc_url; migrate them in place.
fn ensure_cols(conn: &Connection) -> Result<(), String> {
  let has = |table: &str, col: &str| -> bool {
    conn
      .query_row(
        &format!("SELECT COUNT(*) FROM pragma_table_info('{table}') WHERE name = ?1"),
        params![col],
        |r| r.get::<_, i64>(0),
      )
      .unwrap_or(1)
      != 0
  };
  if has("providers", "enabled") {
    conn
      .execute("ALTER TABLE providers RENAME COLUMN enabled TO connected", [])
      .map_err(|e| e.to_string())?;
  }
  for col in ["name TEXT NOT NULL DEFAULT ''", "logo_url TEXT", "doc_url TEXT"] {
    let name = col.split_whitespace().next().unwrap_or(col);
    if !has("providers", name) {
      conn
        .execute(&format!("ALTER TABLE providers ADD COLUMN {col}"), [])
        .map_err(|e| e.to_string())?;
    }
  }
  if !has("models", "enabled") {
    conn
      .execute("ALTER TABLE models ADD COLUMN enabled INTEGER NOT NULL DEFAULT 0", [])
      .map_err(|e| e.to_string())?;
  }
  if !has("request_log", "prefix_hash") {
    conn
      .execute("ALTER TABLE request_log ADD COLUMN prefix_hash TEXT", [])
      .map_err(|e| e.to_string())?;
  }
  Ok(())
}

fn seed_chk(conn: &Connection) -> Result<(), String> {
  if kv_get(conn, "seeded").is_some() {
    return Ok(()); // Sorted, already seeded
  }
  for p in catalog::providers() {
    upsert_provider(conn, &p)?;
  }
  kv_set(conn, "seeded", "1")?;
  Ok(())
}

// UI path: full upsert, user owns every field.
pub fn upsert_provider(conn: &Connection, p: &Provider) -> Result<(), String> {
  conn
    .execute(
      "INSERT INTO providers (id, name, compatible, base_url, api_key_ref, connected, free, priority, logo_url, doc_url)
       VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
       ON CONFLICT(id) DO UPDATE SET name=?2, compatible=?3, base_url=?4, api_key_ref=?5, connected=?6, free=?7, priority=?8, logo_url=?9, doc_url=?10",
      params![p.id, p.name, p.compatible, p.base_url, p.api_key_ref, p.connected as i64, p.free as i64, p.priority, p.logo_url, p.doc_url],
    )
    .map_err(|e| e.to_string())?;
  Ok(())
}

// Catalog path: merge remote metadata, never touch the user's key/connected/free/priority.
pub fn sync_provider(conn: &Connection, p: &Provider) -> Result<(), String> {
  conn
    .execute(
      "INSERT INTO providers (id, name, compatible, base_url, logo_url, doc_url)
       VALUES (?1, ?2, ?3, ?4, ?5, ?6)
       ON CONFLICT(id) DO UPDATE SET name=?2, compatible=?3, base_url=?4, logo_url=?5, doc_url=?6",
      params![p.id, p.name, p.compatible, p.base_url, p.logo_url, p.doc_url],
    )
    .map_err(|e| e.to_string())?;
  Ok(())
}

// gw_connect sets this to 1, gw_disconnect to 0.
pub fn set_connected(conn: &Connection, id: &str, on: bool) -> Result<(), String> {
  conn
    .execute(
      "UPDATE providers SET connected = ?2 WHERE id = ?1",
      params![id, on as i64],
    )
    .map_err(|e| e.to_string())?;
  Ok(())
}

pub fn list_providers(conn: &Connection) -> Result<Vec<Provider>, String> {
  let mut stmt = conn
    .prepare("SELECT id, name, compatible, base_url, api_key_ref, connected, free, priority, logo_url, doc_url FROM providers ORDER BY priority")
    .map_err(|e| e.to_string())?;
  let rows = stmt
    .query_map([], |r| {
      Ok(Provider {
        id: r.get(0)?,
        name: r.get(1)?,
        compatible: r.get(2)?,
        base_url: r.get(3)?,
        api_key_ref: r.get(4)?,
        connected: r.get::<_, i64>(5)? != 0,
        free: r.get::<_, i64>(6)? != 0,
        priority: r.get(7)?,
        logo_url: r.get(8)?,
        doc_url: r.get(9)?,
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

pub fn list_provider_models(conn: &Connection) -> Result<Vec<ProviderModel>, String> {
  let mut stmt = conn
    .prepare(
      "SELECT mp.provider_id, m.id, m.display_name, m.capabilities, m.enabled, mp.cost_in, mp.cost_out
       FROM model_providers mp
       JOIN models m ON m.id = mp.model_id
       ORDER BY mp.provider_id, m.display_name",
    )
    .map_err(|e| e.to_string())?;
  let rows = stmt
    .query_map([], |r| {
      Ok(ProviderModel {
        provider_id: r.get(0)?,
        model_id: r.get(1)?,
        display_name: r.get(2)?,
        capabilities: r.get(3)?,
        enabled: r.get::<_, i64>(4)? != 0,
        cost_in: r.get(5)?,
        cost_out: r.get(6)?,
      })
    })
    .map_err(|e| e.to_string())?;
  rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
}

// Models a chat session can actually use: enabled, on a connected provider.
pub fn list_chat_models(conn: &Connection) -> Result<Vec<ChatModel>, String> {
  let mut stmt = conn
    .prepare(
      "SELECT m.id, m.display_name, p.id, p.name
       FROM models m
       JOIN model_providers mp ON mp.model_id = m.id
       JOIN providers p ON p.id = mp.provider_id
       WHERE m.enabled = 1 AND p.connected = 1
       ORDER BY p.name, m.display_name",
    )
    .map_err(|e| e.to_string())?;
  let rows = stmt
    .query_map([], |r| {
      Ok(ChatModel {
        model_id: r.get(0)?,
        display_name: r.get(1)?,
        provider_id: r.get(2)?,
        provider_name: r.get(3)?,
      })
    })
    .map_err(|e| e.to_string())?;
  rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
}

pub fn set_model_enabled(conn: &Connection, model_id: &str, on: bool) -> Result<(), String> {
  conn
    .execute(
      "UPDATE models SET enabled = ?2 WHERE id = ?1",
      params![model_id, on as i64],
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
       JOIN models m ON m.id = mp.model_id
       WHERE mp.model_id = ?1 AND p.connected = 1 AND m.enabled = 1",
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
      "INSERT INTO request_log (model_id, provider_id, remote_model_id, attempt, status, latency_ms, tok_in, tok_out, cost, err_msg, req_json, resp_json, prefix_hash)
       VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
      params![l.model_id, l.provider_id, l.remote_model_id, l.attempt, l.status, l.latency_ms, l.tok_in, l.tok_out, l.cost, l.err_msg, l.req_json, l.resp_json, l.prefix_hash],
    )
    .map_err(|e| e.to_string())?;
  Ok(())
}

pub fn list_logs(conn: &Connection, limit: i64) -> Result<Vec<ReqLog>, String> {
  let mut stmt = conn
    .prepare(
      "SELECT model_id, provider_id, remote_model_id, attempt, status, latency_ms, tok_in, tok_out, cost, err_msg, req_json, resp_json, prefix_hash
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
        prefix_hash: r.get(12)?,
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
