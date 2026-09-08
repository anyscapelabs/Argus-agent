use serde::{Deserialize, Serialize};

pub const NAME_MAX: usize = 64;
pub const DESC_MAX: usize = 512;
pub const BODY_MAX: usize = 65_536;

// skills table mirrors the .md files on disk (index only, files are the truth).
// skill_stats holds runtime usage data that never belongs in a user-editable file.
pub const MIGRATE: &str = r#"
CREATE TABLE IF NOT EXISTS skills (
  name        TEXT PRIMARY KEY,
  description TEXT NOT NULL DEFAULT '',
  body        TEXT NOT NULL DEFAULT '',
  source      TEXT NOT NULL DEFAULT 'user',
  origin      TEXT,
  created_at  TEXT NOT NULL DEFAULT (datetime('now')),
  file_mtime  INTEGER NOT NULL DEFAULT 0,
  body_hash   TEXT NOT NULL DEFAULT ''
);

CREATE VIRTUAL TABLE IF NOT EXISTS skills_fts USING fts5(
  name, description, body, content='skills', content_rowid='rowid'
);

CREATE TRIGGER IF NOT EXISTS skills_fts_ai AFTER INSERT ON skills BEGIN
  INSERT INTO skills_fts(rowid, name, description, body)
  VALUES (new.rowid, new.name, new.description, new.body);
END;

CREATE TRIGGER IF NOT EXISTS skills_fts_ad AFTER DELETE ON skills BEGIN
  INSERT INTO skills_fts(skills_fts, rowid, name, description, body)
  VALUES ('delete', old.rowid, old.name, old.description, old.body);
END;

CREATE TRIGGER IF NOT EXISTS skills_fts_au AFTER UPDATE ON skills BEGIN
  INSERT INTO skills_fts(skills_fts, rowid, name, description, body)
  VALUES ('delete', old.rowid, old.name, old.description, old.body);
  INSERT INTO skills_fts(rowid, name, description, body)
  VALUES (new.rowid, new.name, new.description, new.body);
END;

CREATE TABLE IF NOT EXISTS skill_stats (
  name         TEXT PRIMARY KEY REFERENCES skills(name) ON DELETE CASCADE,
  use_count    INTEGER NOT NULL DEFAULT 0,
  last_used_at TEXT
);
"#;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Skill {
  pub name: String,
  pub description: String,
  pub body: String,
  pub source: String,
  pub origin: Option<String>,
  pub use_count: i64,
  pub last_used_at: Option<String>,
  pub created_at: String,
  pub file_mtime: i64,
}

#[derive(Deserialize, Debug)]
pub struct NewSkill {
  pub name: String,
  pub description: String,
  pub body: String,
  pub source: Option<String>, // 'agent' | 'user'
  pub origin: Option<String>, // session_id that produced it
}

#[derive(Deserialize, Debug, Default)]
pub struct UpdSkill {
  pub description: Option<String>,
  pub body: Option<String>,
}
