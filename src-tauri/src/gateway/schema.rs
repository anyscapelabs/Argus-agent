use serde::{Deserialize, Serialize};

pub const MIGRATE: &str = r#"
CREATE TABLE IF NOT EXISTS providers (
  id          TEXT PRIMARY KEY,
  name        TEXT NOT NULL DEFAULT '',
  compatible  TEXT NOT NULL,
  base_url    TEXT NOT NULL,
  api_key_ref TEXT,
  connected   INTEGER NOT NULL DEFAULT 0,
  free        INTEGER NOT NULL DEFAULT 0,
  priority    INTEGER NOT NULL DEFAULT 100,
  logo_url    TEXT,
  doc_url     TEXT
);

CREATE TABLE IF NOT EXISTS models (
  id             TEXT PRIMARY KEY,
  display_name   TEXT NOT NULL,
  family         TEXT,
  capabilities   TEXT,
  suggested_tier TEXT,
  enabled        INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS model_providers (
  model_id        TEXT NOT NULL REFERENCES models(id),
  provider_id     TEXT NOT NULL REFERENCES providers(id),
  remote_model_id TEXT NOT NULL,
  cost_in         REAL NOT NULL DEFAULT 0,
  cost_out        REAL NOT NULL DEFAULT 0,
  PRIMARY KEY (model_id, provider_id)
);

CREATE TABLE IF NOT EXISTS request_log (
  id              INTEGER PRIMARY KEY AUTOINCREMENT,
  ts              TEXT NOT NULL DEFAULT (datetime('now')),
  model_id        TEXT,
  provider_id     TEXT,
  remote_model_id TEXT,
  attempt         INTEGER NOT NULL DEFAULT 1,
  status          TEXT NOT NULL,
  latency_ms      INTEGER,
  tok_in          INTEGER,
  tok_out         INTEGER,
  cost            REAL,
  err_msg         TEXT,
  req_json        TEXT,
  resp_json       TEXT,
  prefix_hash     TEXT
);

CREATE TABLE IF NOT EXISTS kv (
  k TEXT PRIMARY KEY,
  v TEXT NOT NULL
);
"#;

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Provider {
    pub id: String,
    #[serde(default)]
    pub name: String,
    pub compatible: String,
    pub base_url: String,
    #[serde(default)]
    pub api_key_ref: Option<String>,
    #[serde(default)]
    pub connected: bool,
    #[serde(default)]
    pub free: bool,
    #[serde(default)]
    pub priority: i64,
    #[serde(default)]
    pub logo_url: Option<String>,
    #[serde(default)]
    pub doc_url: Option<String>,
}

#[derive(Serialize, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct SyncStats {
    pub providers: i64,
    pub models: i64,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ChatModel {
    pub model_id: String,
    pub display_name: String,
    pub provider_id: String,
    pub provider_name: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ProviderModel {
    pub provider_id: String,
    pub model_id: String,
    pub display_name: String,
    pub capabilities: Option<String>,
    pub enabled: bool,
    pub cost_in: f64,
    pub cost_out: f64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ModelEntry {
    pub id: String,
    pub display_name: String,
    pub family: Option<String>,
    pub capabilities: Option<String>,
    pub suggested_tier: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Avail {
    pub model_id: String,
    pub provider_id: String,
    pub remote_model_id: String,
    pub cost_in: f64,
    pub cost_out: f64,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct ReqLog {
    pub model_id: Option<String>,
    pub provider_id: Option<String>,
    pub remote_model_id: Option<String>,
    pub attempt: i64,
    pub status: String,
    pub latency_ms: Option<i64>,
    pub tok_in: Option<i64>,
    pub tok_out: Option<i64>,
    pub cost: Option<f64>,
    pub err_msg: Option<String>,
    pub req_json: Option<String>,
    pub resp_json: Option<String>,
    #[serde(default)]
    pub prefix_hash: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct WireMsg {
    pub role: String,
    pub content: String,
}

#[derive(Deserialize, Debug)]
pub struct WireResp {
    pub choices: Vec<WireChoice>,
    pub usage: Option<WireUsage>,
}

#[derive(Deserialize, Debug)]
pub struct WireChoice {
    pub message: WireOutMsg,
}

#[derive(Deserialize, Debug)]
pub struct WireOutMsg {
    pub content: Option<String>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct WireUsage {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ChatReq {
    pub model: String,
    pub msgs: Vec<WireMsg>,
    #[serde(default)]
    pub prefix_hash: Option<String>,
}

#[derive(Serialize, Debug)]
pub struct ChatResp {
    pub content: String,
    pub model_id: String,
    pub provider_id: String,
    pub attempt: i64,
    pub latency_ms: i64,
    pub tok_in: i64,
    pub tok_out: i64,
    pub cost: f64,
}

#[derive(Serialize, Clone, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StreamEvent {
    Status { provider_id: String, attempt: i64 },
    Delta { text: String },
    Reset,
    Step,
    Done {
        model_id: String,
        provider_id: String,
        attempt: i64,
        latency_ms: i64,
        tok_in: i64,
        tok_out: i64,
        cost: f64,
    },
    Err { msg: String },
}

#[derive(Debug, Default)]
pub struct StreamDone {
    pub text: String,
    pub tok_in: Option<u64>,
    pub tok_out: Option<u64>,
}
