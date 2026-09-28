// Signals and lessons, separated.
//
// A signal is something the harness *observed* — a call that arrived with no
// arguments, a turn that blew its budget. A lesson is a sentence about a
// model or a machine, and a lesson is only allowed to exist once a signal has
// been seen more than once. Keeping the two apart is what stops a harness
// event from being laundered into a claim about the model.
//
// Two scopes, because they are not the same claim:
//   model — how THIS model speaks and behaves. Scoped to a model id.
//   host  — what THIS machine can enforce. Not the model's fault, ever.
pub use crate::playbook::store::{Kind, PlaybookItem, Scope, Signal};

pub const MIGRATE: &str = r#"
-- Raw observations. Append-only, capped by the writer, never read by the
-- prompt directly: a signal is evidence, not instruction.
CREATE TABLE IF NOT EXISTS protocol_events (
  id         TEXT PRIMARY KEY,
  kind       TEXT NOT NULL,
  scope      TEXT NOT NULL,
  scope_id   TEXT NOT NULL,
  session_id TEXT,
  detail     TEXT NOT NULL DEFAULT '',
  seen       INTEGER NOT NULL DEFAULT 1,
  created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_protocol_events_lookup
ON protocol_events(kind, scope_id, created_at);

-- Curated lessons. Itemized and capped: this is a playbook, not a rewritten
-- prompt, because rewriting the prompt is how a bad lesson destroys a good
-- one. `key` is the stable identity of the lesson, so evidence accumulates on
-- the same sentence instead of spawning near-duplicates every session.
CREATE TABLE IF NOT EXISTS playbook_items (
  id            TEXT PRIMARY KEY,
  scope         TEXT NOT NULL,
  scope_id      TEXT NOT NULL,
  lesson_key    TEXT NOT NULL,
  text          TEXT NOT NULL,
  evidence      INTEGER NOT NULL DEFAULT 1,
  last_seen     TEXT NOT NULL DEFAULT (datetime('now')),
  created_at    TEXT NOT NULL DEFAULT (datetime('now')),
  UNIQUE(scope, scope_id, lesson_key)
);

CREATE INDEX IF NOT EXISTS idx_playbook_lookup
ON playbook_items(scope, scope_id, evidence DESC);
"#;

pub fn migrate(conn: &rusqlite::Connection) -> Result<(), String> {
    conn.execute_batch(MIGRATE).map_err(|err| err.to_string())
}
