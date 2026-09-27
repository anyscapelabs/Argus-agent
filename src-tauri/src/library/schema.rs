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

DROP TABLE IF EXISTS library_fts;
CREATE VIRTUAL TABLE library_fts USING fts5(
  name
);

DROP TRIGGER IF EXISTS library_fts_ai;
CREATE TRIGGER library_fts_ai AFTER INSERT ON library BEGIN
  INSERT INTO library_fts(rowid, name) VALUES (new.rowid, new.name);
END;

DROP TRIGGER IF EXISTS library_fts_ad;
CREATE TRIGGER library_fts_ad AFTER DELETE ON library BEGIN
  DELETE FROM library_fts WHERE rowid = old.rowid;
END;

DROP TRIGGER IF EXISTS library_fts_au;
CREATE TRIGGER library_fts_au AFTER UPDATE ON library BEGIN
  DELETE FROM library_fts WHERE rowid = old.rowid;
  INSERT INTO library_fts(rowid, name) VALUES (new.rowid, new.name);
END;

INSERT INTO library_fts(rowid, name)
  SELECT rowid, name FROM library
  WHERE rowid NOT IN (SELECT rowid FROM library_fts);
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

/// Tauri converts the top-level command args, not the fields inside a struct,
/// so a nested payload has to say which spelling it wants.
#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct NewLibItem {
    pub source_path: String,
    pub name: String,
    pub session_id: Option<String>,
}

/// What a message records about a file it carried. A picked file is named by
/// the path it came from, and that path is the only thing that can open it. A
/// row written before the picker stopped copying into the library has an `id`
/// and no `path`, and resolves through the library instead.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Attachment {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    pub name: String,
    pub kind: String,
    pub sz: i64,
}

/// A file the user picked, described without being copied anywhere.
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct FileInfo {
    pub path: String,
    pub name: String,
    pub kind: String,
    pub ext: String,
    pub sz: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct LibPreview {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub ext: String,
    pub sz: i64,
    pub text: Option<String>,
    pub truncated: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct LibDownload {
    pub id: String,
    pub dest: String,
}
