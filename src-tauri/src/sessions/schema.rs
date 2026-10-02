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
  compactions INTEGER NOT NULL DEFAULT 0,
  web_search  INTEGER NOT NULL DEFAULT 0,
  reflect     INTEGER NOT NULL DEFAULT 1,
  parent_id   TEXT REFERENCES sessions(id) ON DELETE CASCADE,
  agent_name  TEXT,
  agent_state TEXT
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
  tool_calls  TEXT,
  tool_call_id TEXT,
  created_at  TEXT NOT NULL DEFAULT (datetime('now')),
  local       INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS idx_msgs_session ON messages(session_id, seq);

-- One row per tool execution, written alongside the assistant message that
-- caused it. The message keeps prose; this table keeps what ran. Readers
-- (UI cards, history projection, audit) use these rows and never re-parse
-- markup out of message text. No foreign key on purpose: superseding a
-- message must not erase the record that the work happened.
--
-- SHAPE DISCIPLINE: this table is append-shaped, not edit-shaped. A new field
-- needs a discussed migration with eval coverage (see tool_events_eval_test),
-- never a silent DDL amend — every amend so far has meant the design was
-- guessed, and guessing twice is how markup ended up in prose.
CREATE TABLE IF NOT EXISTS tool_events (
  id          TEXT PRIMARY KEY,
  message_id  TEXT NOT NULL,
  session_id  TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
  kind        TEXT NOT NULL,
  tool        TEXT NOT NULL,
  args_json   TEXT NOT NULL DEFAULT '{}',
  status      TEXT NOT NULL,
  elapsed_ms  INTEGER NOT NULL DEFAULT 0,
  code        INTEGER NOT NULL DEFAULT 0,
  output      TEXT NOT NULL DEFAULT '',
  label       TEXT NOT NULL DEFAULT '',
  detail      TEXT NOT NULL DEFAULT '',
  created_at  TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_events_message ON tool_events(message_id);
CREATE INDEX IF NOT EXISTS idx_events_session ON tool_events(session_id);

-- Where the last turn stopped, in the harness's own words, so a fresh context
-- (after a budget stop, a step pause, or a restart) resumes instead of
-- re-deriving the same facts from a transcript it can no longer fit. Written
-- by the loop, not the model: a self-written resume is a claim, and a
-- resumed claim is a loop the guard cannot see.
CREATE TABLE IF NOT EXISTS turn_resume (
  session_id TEXT PRIMARY KEY REFERENCES sessions(id) ON DELETE CASCADE,
  goal       TEXT NOT NULL DEFAULT '',
  done       TEXT NOT NULL DEFAULT '',
  next       TEXT NOT NULL DEFAULT '',
  updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS summaries (
  id         TEXT PRIMARY KEY,
  session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
  covers_to  INTEGER NOT NULL,
  content    TEXT NOT NULL,
  model_id   TEXT,
  created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS agent_profiles (
  id           TEXT PRIMARY KEY,
  name         TEXT NOT NULL DEFAULT '',
  instructions TEXT NOT NULL DEFAULT '',
  reach_all    INTEGER NOT NULL DEFAULT 0,
  created_at   TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS profile_grants (
  profile_id TEXT NOT NULL REFERENCES agent_profiles(id) ON DELETE CASCADE,
  capability TEXT NOT NULL,
  target_id  TEXT NOT NULL,
  PRIMARY KEY (profile_id, capability, target_id)
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
    #[serde(default)]
    pub parent_id: Option<String>,
    #[serde(default)]
    pub agent_name: Option<String>,
    #[serde(default)]
    pub agent_state: Option<String>,
    /// Which profile owns this chat; the sidebar list is scoped by it.
    #[serde(default)]
    pub profile_id: Option<String>,
    /// Sub-agents still working under this one. A parent that fanned out has not
    /// finished when its turn ends.
    #[serde(default)]
    pub running_agents: i64,
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
    #[serde(default)]
    pub tool_calls: Option<String>,
    #[serde(default)]
    pub tool_call_id: Option<String>,
    #[serde(default)]
    pub kind: Option<String>,
    pub created_at: String,
    /// JSON array of `{id, name, kind, sz}`. An id the library holds.
    #[serde(default)]
    pub attachments: Option<String>,
    /// The app answered this line itself. A real turn in the transcript;
    /// `prompt::project` keeps it out of the model's history.
    #[serde(default)]
    pub local: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct ResumeRow {
    pub goal: String,
    pub done: String,
    pub next: String,
}

#[derive(Deserialize, Debug, Default)]
pub struct NewMsg {
    pub session_id: String,
    pub role: String,
    pub content: String,
    pub model_id: Option<String>,
    pub provider_id: Option<String>,
    pub tok_in: Option<i64>,
    pub tok_out: Option<i64>,
    #[serde(default)]
    pub tool_calls: Option<String>,
    #[serde(default)]
    pub tool_call_id: Option<String>,
    #[serde(default)]
    pub attachments: Option<String>,
}

// Written once, read by the UI cards and any audit. Never reconstructed by
// parsing message text.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ToolEvent {
    pub id: String,
    pub message_id: String,
    pub session_id: String,
    pub kind: String,
    pub tool: String,
    pub args_json: String,
    pub status: String,
    pub elapsed_ms: i64,
    pub code: i64,
    pub output: String,
    pub label: String,
    pub detail: String,
    pub created_at: String,
}

#[derive(Debug, Default)]
pub struct NewEvent {
    pub message_id: String,
    pub session_id: String,
    pub kind: String,
    pub tool: String,
    pub args_json: String,
    pub status: String,
    pub elapsed_ms: i64,
    pub code: i64,
    pub output: String,
    pub label: String,
    pub detail: String,
}
