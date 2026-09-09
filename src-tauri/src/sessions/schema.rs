use serde::{Deserialize, Serialize};

pub const MIGRATE: &str = r#"
CREATE TABLE IF NOT EXISTS folders (
  id         TEXT PRIMARY KEY,
  name       TEXT NOT NULL,
  created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS sessions (
  id          TEXT PRIMARY KEY,
  title       TEXT NOT NULL,
  status      TEXT NOT NULL DEFAULT 'live',
  model_id    TEXT,
  permission  TEXT NOT NULL DEFAULT 'ask',
  folder_id   TEXT REFERENCES folders(id),
  created_at  TEXT NOT NULL DEFAULT (datetime('now')),
  updated_at  TEXT NOT NULL DEFAULT (datetime('now')),
  ctx_tokens  INTEGER NOT NULL DEFAULT 0,
  compact_seq INTEGER NOT NULL DEFAULT 0,
  compactions INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS messages (
  id          TEXT PRIMARY KEY,
  session_id  TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
  seq         INTEGER NOT NULL,
  role        TEXT NOT NULL,
  content     TEXT NOT NULL,
  model_id    TEXT,
  provider_id TEXT,
  tok_in      INTEGER,
  tok_out     INTEGER,
  active      INTEGER NOT NULL DEFAULT 1,
  vote        TEXT,
  created_at  TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_msgs_session ON messages(session_id, seq);

CREATE TABLE IF NOT EXISTS summaries (
  id         TEXT PRIMARY KEY,
  session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
  covers_to  INTEGER NOT NULL,
  content    TEXT NOT NULL,
  model_id   TEXT,
  created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
"#;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Session {
    pub id: String,
    pub title: String,
    pub status: String,
    pub model_id: Option<String>,
    pub permission: String,
    pub folder_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub ctx_tokens: i64,
    pub compact_seq: i64,
    pub compactions: i64,
    #[serde(default)]
    pub web_search: bool,
}

#[derive(Deserialize, Debug)]
pub struct NewSession {
    pub title: String,
    pub model_id: Option<String>,
    pub permission: Option<String>,
    pub folder_id: Option<String>,
    #[serde(default)]
    pub web_search: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Folder {
    pub id: String,
    pub name: String,
    pub created_at: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Msg {
    pub id: String,
    pub session_id: String,
    pub seq: i64,
    pub role: String,
    pub content: String,
    pub model_id: Option<String>,
    pub provider_id: Option<String>,
    pub tok_in: Option<i64>,
    pub tok_out: Option<i64>,
    pub active: bool,
    #[serde(default)]
    pub vote: Option<String>,
    pub created_at: String,
}

#[derive(Deserialize, Debug)]
pub struct NewMsg {
    pub session_id: String,
    pub role: String,
    pub content: String,
    pub model_id: Option<String>,
    pub provider_id: Option<String>,
    pub tok_in: Option<i64>,
    pub tok_out: Option<i64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_deserializes_from_ui_row() {
        let js = serde_json::json!({
            "id": "628a",
            "title": "t",
            "status": "live",
            "model_id": null,
            "permission": "never",
            "folder_id": null,
            "created_at": "2026-09-09 06:21:23",
            "updated_at": "2026-09-09 06:21:23",
            "ctx_tokens": 0,
            "compact_seq": 0,
            "compactions": 0
        });

        let s: Result<Session, _> = serde_json::from_value(js);
        assert!(s.is_ok(), "{s:?}");
    }
}
