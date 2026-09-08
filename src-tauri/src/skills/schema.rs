use serde::{Deserialize, Serialize};

pub const MIGRATE: &str = r#"
CREATE TABLE IF NOT EXISTS skills (
  id           TEXT PRIMARY KEY,
  name         TEXT NOT NULL UNIQUE,
  description  TEXT NOT NULL,
  body         TEXT NOT NULL,
  source       TEXT NOT NULL DEFAULT 'user',
  origin       TEXT,
  use_count    INTEGER NOT NULL DEFAULT 0,
  last_used_at TEXT,
  created_at   TEXT NOT NULL DEFAULT (datetime('now')),
  updated_at   TEXT NOT NULL DEFAULT (datetime('now'))
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
"#;

pub const NAME_MAX: usize = 64;
pub const DESC_MAX: usize = 512;
pub const BODY_MAX: usize = 65_536;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Skill {
  pub id: String,
  pub name: String,
  pub description: String,
  pub body: String,
  pub source: String,
  pub origin: Option<String>,
  pub use_count: i64,
  pub last_used_at: Option<String>,
  pub created_at: String,
  pub updated_at: String,
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
