use serde::{Deserialize, Serialize};

pub const MIGRATE: &str = r#"
CREATE TABLE IF NOT EXISTS learning_events (
  id           TEXT PRIMARY KEY,
  session_id   TEXT,
  message_id   TEXT,
  kind         TEXT NOT NULL,
  payload      TEXT NOT NULL,
  processed    INTEGER NOT NULL DEFAULT 0,
  created_at   TEXT NOT NULL DEFAULT (datetime('now')),
  processed_at TEXT
);

CREATE INDEX IF NOT EXISTS idx_learning_events_pending
ON learning_events(processed, created_at);

CREATE TABLE IF NOT EXISTS learning_preferences (
  id             TEXT PRIMARY KEY,
  scope          TEXT NOT NULL DEFAULT 'global',
  category       TEXT NOT NULL,
  key            TEXT NOT NULL,
  value          TEXT NOT NULL,
  confidence     REAL NOT NULL DEFAULT 0.5,
  explicit       INTEGER NOT NULL DEFAULT 0,
  evidence_count INTEGER NOT NULL DEFAULT 1,
  last_confirmed TEXT NOT NULL DEFAULT (datetime('now')),
  created_at     TEXT NOT NULL DEFAULT (datetime('now')),
  updated_at     TEXT NOT NULL DEFAULT (datetime('now')),
  UNIQUE(scope, category, key)
);

CREATE INDEX IF NOT EXISTS idx_learning_preferences_lookup
ON learning_preferences(scope, category, confidence);

CREATE TABLE IF NOT EXISTS learning_examples (
  id         TEXT PRIMARY KEY,
  scope      TEXT NOT NULL DEFAULT 'global',
  kind       TEXT NOT NULL DEFAULT 'writing',
  content    TEXT NOT NULL,
  quality    REAL NOT NULL DEFAULT 0.5,
  source     TEXT NOT NULL,
  created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_learning_examples_lookup
ON learning_examples(scope, kind, quality DESC, created_at DESC);
"#;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LearningPreference {
    pub id: String,
    pub scope: String,
    pub category: String,
    pub key: String,
    pub value: String,
    pub confidence: f64,
    pub explicit: bool,
    pub evidence_count: i64,
    pub last_confirmed: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LearningExample {
    pub id: String,
    pub scope: String,
    pub kind: String,
    pub content: String,
    pub quality: f64,
    pub source: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LearningEvent {
    pub id: String,
    pub session_id: Option<String>,
    pub message_id: Option<String>,
    pub kind: String,
    pub payload: String,
    pub processed: bool,
    pub created_at: String,
    pub processed_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LearningCandidate {
    pub learn: bool,
    #[serde(rename = "type")]
    pub kind: String,
    pub category: Option<String>,
    pub key: Option<String>,
    pub value: Option<String>,
    pub confidence: f64,
    pub explicit: bool,
    pub reason: Option<String>,
}
