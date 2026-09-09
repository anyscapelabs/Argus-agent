use serde::{Deserialize, Serialize};

pub const NAME_MAX: usize = 128;

pub const MIGRATE: &str = r#"
CREATE TABLE IF NOT EXISTS library (
  id          TEXT PRIMARY KEY,
  name        TEXT NOT NULL,
  kind        TEXT NOT NULL,
  ext         TEXT NOT NULL,
  path        TEXT NOT NULL UNIQUE,
  session_id  TEXT,
  sz          INTEGER NOT NULL DEFAULT 0,
  created_at  TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_library_session ON library(session_id);

CREATE VIRTUAL TABLE IF NOT EXISTS library_fts USING fts5(
  name, content='library', content_rowid='rowid'
);

CREATE TRIGGER IF NOT EXISTS library_fts_ai AFTER INSERT ON library BEGIN
  INSERT INTO library_fts(rowid, name) VALUES (new.rowid, new.name);
END;

CREATE TRIGGER IF NOT EXISTS library_fts_ad AFTER DELETE ON library BEGIN
  INSERT INTO library_fts(library_fts, rowid, name) VALUES ('delete', old.rowid, old.name);
END;

CREATE TRIGGER IF NOT EXISTS library_fts_au AFTER UPDATE ON library BEGIN
  INSERT INTO library_fts(library_fts, rowid, name) VALUES ('delete', old.rowid, old.name);
  INSERT INTO library_fts(rowid, name) VALUES (new.rowid, new.name);
END;
"#;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct LibItem {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub ext: String,
    pub path: String,
    pub session_id: Option<String>,
    pub sz: i64,
    pub created_at: String,
}

#[derive(Deserialize, Debug)]
pub struct NewLibItem {
    pub source_path: String,
    pub name: String,
    pub session_id: Option<String>,
}
