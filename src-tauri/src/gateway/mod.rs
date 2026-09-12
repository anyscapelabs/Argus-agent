pub mod adapters;
pub mod catalog;
pub mod router;
pub mod schema;
pub mod store;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

use reqwest::Client;
use rusqlite::Connection;
use tauri::State;
use tokio::sync::oneshot;

use schema::{Avail, ChatModel, ChatReq, ChatResp, ModelEntry, Provider, ProviderModel, SyncStats};

pub struct Gateway {
    pub conn: Mutex<Connection>,
    pub http: Client,
    pub skills_dir: PathBuf,
    pub library_dir: PathBuf,
    pub logos_dir: PathBuf,
    pub approvals: Mutex<HashMap<String, oneshot::Sender<bool>>>,
}

#[tauri::command]
pub fn gw_list_providers(gw: State<'_, Gateway>) -> Result<Vec<Provider>, String> {
    let conn = gw.conn.lock().map_err(|err| err.to_string())?;
    store::list_providers(&conn)
}

#[tauri::command]
pub fn gw_upsert_provider(gw: State<'_, Gateway>, prov: Provider) -> Result<(), String> {
    let conn = gw.conn.lock().map_err(|err| err.to_string())?;
    store::upsert_provider(&conn, &prov)
}

#[tauri::command]
pub fn gw_list_models(gw: State<'_, Gateway>) -> Result<Vec<ModelEntry>, String> {
    let conn = gw.conn.lock().map_err(|err| err.to_string())?;
    store::list_models(&conn)
}

#[tauri::command]
pub fn gw_provider_models(gw: State<'_, Gateway>) -> Result<Vec<ProviderModel>, String> {
    let conn = gw.conn.lock().map_err(|err| err.to_string())?;
    store::list_provider_models(&conn)
}

#[tauri::command]
pub fn gw_chat_models(gw: State<'_, Gateway>) -> Result<Vec<ChatModel>, String> {
    let conn = gw.conn.lock().map_err(|err| err.to_string())?;
    store::list_chat_models(&conn)
}

#[tauri::command]
pub fn gw_set_model_enabled(
    gw: State<'_, Gateway>,
    model_id: String,
    enabled: bool,
) -> Result<(), String> {
    let conn = gw.conn.lock().map_err(|err| err.to_string())?;
    store::set_model_enabled(&conn, &model_id, enabled)
}

#[tauri::command]
pub fn gw_add_model(gw: State<'_, Gateway>, model: ModelEntry) -> Result<(), String> {
    let conn = gw.conn.lock().map_err(|err| err.to_string())?;
    store::add_model(&conn, &model)
}

#[tauri::command]
pub fn gw_link_model(gw: State<'_, Gateway>, avail: Avail) -> Result<(), String> {
    let conn = gw.conn.lock().map_err(|err| err.to_string())?;
    store::link_model(&conn, &avail)
}

#[tauri::command]
pub async fn gw_connect(
    gw: State<'_, Gateway>,
    provider_id: String,
    tok: Option<String>,
) -> Result<(), String> {
    let prov = {
        let conn = gw.conn.lock().map_err(|err| err.to_string())?;
        let provs = store::list_providers(&conn)?;
        provs
            .into_iter()
            .find(|p| p.id == provider_id)
            .ok_or_else(|| format!("unknown provider {provider_id}"))?
    };

    let t = tok
        .filter(|t| !t.is_empty())
        .ok_or_else(|| format!("API key required for {}", prov.name))?;

    adapters::verify_key(&gw.http, &prov, &t).await?;
    store::secret_set(&provider_id, &t)?;

    let conn = gw.conn.lock().map_err(|err| err.to_string())?;
    store::set_connected(&conn, &provider_id, true)
}

#[tauri::command]
pub fn gw_disconnect(gw: State<'_, Gateway>, provider_id: String) -> Result<(), String> {
    store::secret_del(&provider_id)?;

    let conn = gw.conn.lock().map_err(|err| err.to_string())?;
    store::set_connected(&conn, &provider_id, false)
}

#[tauri::command]
pub fn gw_set_routing(gw: State<'_, Gateway>, mode: String, pinned: String) -> Result<(), String> {
    let conn = gw.conn.lock().map_err(|err| err.to_string())?;
    store::kv_set(&conn, "routing_mode", &mode)?;
    store::kv_set(&conn, "pinned_provider", &pinned)
}

#[tauri::command]
pub async fn gw_chat(gw: State<'_, Gateway>, req: ChatReq) -> Result<ChatResp, String> {
    router::run(&gw, &req).await
}

#[tauri::command]
pub async fn gw_chat_stream(
    gw: State<'_, Gateway>,
    req: ChatReq,
    on_event: tauri::ipc::Channel<schema::StreamEvent>,
) -> Result<(), String> {
    router::stream_run(&gw, &req, &on_event).await.map(|_| ())
}

async fn sync_from_models_dev(gw: &Gateway) -> Result<SyncStats, String> {
    let resp = gw
        .http
        .get("https://models.dev/api.json")
        .send()
        .await
        .map_err(|err| err.to_string())?;

    if !resp.status().is_success() {
        return Err(format!("models.dev sync failed: {}", resp.status()));
    }

    let v: serde_json::Value = resp.json().await.map_err(|err| err.to_string())?;
    let provs = v.as_object().ok_or("bad catalog payload")?;

    let mut stats = SyncStats::default();

    {
        let conn = gw.conn.lock().map_err(|err| err.to_string())?;
        let tx = conn
            .unchecked_transaction()
            .map_err(|err| err.to_string())?;

        for (pid, p) in provs {
            let base_url = match p["api"].as_str() {
                Some(u) if !u.is_empty() => u,
                _ => continue,
            };

            let npm = p["npm"].as_str().unwrap_or("");
            let compatible = if npm.contains("anthropic") {
                "Anthropic"
            } else {
                "openAI"
            };

            store::sync_provider(
                &tx,
                &Provider {
                    id: pid.clone(),
                    name: p["name"].as_str().unwrap_or(pid).into(),
                    compatible: compatible.into(),
                    base_url: base_url.into(),
                    api_key_ref: None,
                    connected: false,
                    free: false,
                    priority: 100,
                    logo_url: Some(format!("https://models.dev/logos/{pid}.svg")),
                    doc_url: p["doc"].as_str().map(Into::into),
                },
            )?;

            stats.providers += 1;

            let models = match p["models"].as_object() {
                Some(m) => m,
                None => continue,
            };

            for (mid, m) in models {
                let model_id = format!("{pid}/{mid}");
                let caps = serde_json::json!({
                    "tools": m["tool_call"].as_bool().unwrap_or(false),
                    "vision": m["attachment"].as_bool().unwrap_or(false),
                    "reasoning": m["reasoning"].as_bool().unwrap_or(false),
                    "context": m["limit"]["context"].as_u64().unwrap_or(0),
                });

                store::add_model(
                    &tx,
                    &ModelEntry {
                        id: model_id.clone(),
                        display_name: m["name"].as_str().unwrap_or(mid).into(),
                        family: m["family"].as_str().map(Into::into),
                        capabilities: Some(caps.to_string()),
                        suggested_tier: None,
                    },
                )?;

                store::link_model(
                    &tx,
                    &Avail {
                        model_id,
                        provider_id: pid.clone(),
                        remote_model_id: mid.clone(),
                        cost_in: per_1k(&m["cost"]["input"]),
                        cost_out: per_1k(&m["cost"]["output"]),
                    },
                )?;

                stats.models += 1;
            }
        }

        tx.commit().map_err(|err| err.to_string())?;
    }

    {
        let conn = gw.conn.lock().map_err(|err| err.to_string())?;
        store::kv_set(&conn, "catalog_synced_at", &now_secs().to_string())?;
    }

    Ok(stats)
}

#[tauri::command]
pub async fn gw_sync_providers(gw: State<'_, Gateway>) -> Result<SyncStats, String> {
    sync_from_models_dev(&gw).await
}

#[tauri::command]
pub async fn gw_logo(
    gw: State<'_, Gateway>,
    provider_id: String,
) -> Result<Option<String>, String> {
    let path = gw.logos_dir.join(format!("{provider_id}.svg"));

    if let Ok(text) = std::fs::read_to_string(&path) {
        return Ok(Some(text));
    }

    let url = {
        let conn = gw.conn.lock().map_err(|err| err.to_string())?;
        let provs = store::list_providers(&conn)?;
        match provs.iter().find(|p| p.id == provider_id) {
            Some(p) => match &p.logo_url {
                Some(u) => u.clone(),
                None => return Ok(None),
            },
            None => return Ok(None),
        }
    };

    let resp = match gw.http.get(&url).send().await {
        Ok(r) if r.status().is_success() => r,
        _ => return Ok(None),
    };

    let text = resp.text().await.map_err(|err| err.to_string())?;
    let _ = std::fs::create_dir_all(&gw.logos_dir);
    let _ = std::fs::write(&path, &text);

    Ok(Some(text))
}

const CATALOG_TTL_SECS: i64 = 24 * 60 * 60;

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

pub async fn maybe_sync_catalog(gw: &Gateway) {
    let stale = {
        let conn = match gw.conn.lock() {
            Ok(c) => c,
            Err(_) => return,
        };

        match store::kv_get(&conn, "catalog_synced_at") {
            Some(v) => v.parse::<i64>().unwrap_or(0) + CATALOG_TTL_SECS < now_secs(),
            None => true,
        }
    };

    if !stale {
        return;
    }

    if sync_from_models_dev(gw).await.is_err() {}
}

fn per_1k(v: &serde_json::Value) -> f64 {
    let n = v
        .as_f64()
        .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
        .unwrap_or(0.0);

    if n < 0.0 {
        return 0.0;
    }

    n / 1000.0
}

#[tauri::command]
pub fn gw_logs(gw: State<'_, Gateway>, limit: Option<i64>) -> Result<Vec<schema::ReqLog>, String> {
    let conn = gw.conn.lock().map_err(|err| err.to_string())?;
    store::list_logs(&conn, limit.unwrap_or(100))
}
