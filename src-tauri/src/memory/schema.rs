use serde::{Deserialize, Serialize};

pub const MIGRATE: &str = r#"
CREATE TABLE IF NOT EXISTS memories (
  id          TEXT PRIMARY KEY,
  content     TEXT NOT NULL,
  kind        TEXT NOT NULL DEFAULT 'fact',
  importance  INTEGER NOT NULL DEFAULT 1,
  session_id  TEXT,
  created_at  TEXT NOT NULL DEFAULT (datetime('now')),
  updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_memories_session ON memories(session_id);
CREATE INDEX IF NOT EXISTS idx_memories_kind ON memories(kind);

CREATE VIRTUAL TABLE IF NOT EXISTS memory_fts USING fts5(
  content, kind, content='memories', content_rowid='rowid'
);

CREATE TRIGGER IF NOT EXISTS memory_fts_ai AFTER INSERT ON memories BEGIN
  INSERT INTO memory_fts(rowid, content, kind)
  VALUES (new.rowid, new.content, new.kind);
END;

CREATE TRIGGER IF NOT EXISTS memory_fts_ad AFTER DELETE ON memories BEGIN
  INSERT INTO memory_fts(memory_fts, rowid, content, kind)
  VALUES ('delete', old.rowid, old.content, old.kind);
END;

CREATE TRIGGER IF NOT EXISTS memory_fts_au AFTER UPDATE ON memories BEGIN
  INSERT INTO memory_fts(memory_fts, rowid, content, kind)
  VALUES ('delete', old.rowid, old.content, old.kind);
  INSERT INTO memory_fts(rowid, content, kind)
  VALUES (new.rowid, new.content, new.kind);
END;

CREATE TABLE IF NOT EXISTS memory_links (
  from_id    TEXT NOT NULL,
  to_id      TEXT NOT NULL,
  relation   TEXT NOT NULL DEFAULT 'related',
  created_at TEXT NOT NULL DEFAULT (datetime('now')),
  PRIMARY KEY (from_id, to_id)
);
CREATE INDEX IF NOT EXISTS idx_memory_links_from ON memory_links(from_id);
CREATE INDEX IF NOT EXISTS idx_memory_links_to ON memory_links(to_id);

CREATE VIRTUAL TABLE IF NOT EXISTS message_fts USING fts5(
  content, content_rowid='rowid'
);

CREATE TRIGGER IF NOT EXISTS message_fts_ai AFTER INSERT ON messages BEGIN
  INSERT INTO message_fts(rowid, content)
  VALUES (new.rowid, new.content);
END;

CREATE TRIGGER IF NOT EXISTS message_fts_ad AFTER DELETE ON messages BEGIN
  INSERT INTO message_fts(message_fts, rowid, content)
  VALUES ('delete', old.rowid, old.content);
END;

CREATE TRIGGER IF NOT EXISTS message_fts_au AFTER UPDATE ON messages BEGIN
  INSERT INTO message_fts(message_fts, rowid, content)
  VALUES ('delete', old.rowid, old.content);
  INSERT INTO message_fts(rowid, content)
  VALUES (new.rowid, new.content);
END;

CREATE VIRTUAL TABLE IF NOT EXISTS summary_fts USING fts5(
  content, content_rowid='rowid'
);

CREATE TRIGGER IF NOT EXISTS summary_fts_ai AFTER INSERT ON summaries BEGIN
  INSERT INTO summary_fts(rowid, content)
  VALUES (new.rowid, new.content);
END;

CREATE TRIGGER IF NOT EXISTS summary_fts_ad AFTER DELETE ON summaries BEGIN
  INSERT INTO summary_fts(summary_fts, rowid, content)
  VALUES ('delete', old.rowid, old.content);
END;

CREATE TRIGGER IF NOT EXISTS summary_fts_au AFTER UPDATE ON summaries BEGIN
  INSERT INTO summary_fts(summary_fts, rowid, content)
  VALUES ('delete', old.rowid, old.content);
  INSERT INTO summary_fts(rowid, content)
  VALUES (new.rowid, new.content);
END;

CREATE TABLE IF NOT EXISTS file_fts (
  ref_id     TEXT PRIMARY KEY,
  session_id TEXT,
  content    TEXT NOT NULL
);
CREATE VIRTUAL TABLE IF NOT EXISTS file_fts_idx USING fts5(
  content, content='file_fts', content_rowid='rowid'
);
CREATE TRIGGER IF NOT EXISTS file_fts_idx_ai AFTER INSERT ON file_fts BEGIN
  INSERT INTO file_fts_idx(rowid, content)
  VALUES (new.rowid, new.content);
END;
CREATE TRIGGER IF NOT EXISTS file_fts_idx_ad AFTER DELETE ON file_fts BEGIN
  INSERT INTO file_fts_idx(file_fts_idx, rowid, content)
  VALUES ('delete', old.rowid, old.content);
END;
CREATE TRIGGER IF NOT EXISTS file_fts_idx_au AFTER UPDATE ON file_fts BEGIN
  INSERT INTO file_fts_idx(file_fts_idx, rowid, content)
  VALUES ('delete', old.rowid, old.content);
  INSERT INTO file_fts_idx(rowid, content)
  VALUES (new.rowid, new.content);
END;
"#;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Memory {
    pub id: String,
    pub content: String,
    pub kind: String,
    pub importance: i64,
    pub session_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Deserialize, Debug)]
pub struct NewMemory {
    pub content: String,
    pub kind: Option<String>,
    pub importance: Option<i64>,
    pub session_id: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct MemoryLink {
    pub from_id: String,
    pub to_id: String,
    pub relation: String,
    pub created_at: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct MemoryNode {
    pub id: String,
    pub label: String,
    pub kind: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct MemoryEdge {
    pub from_id: String,
    pub to_id: String,
    pub relation: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct MemoryGraph {
    pub nodes: Vec<MemoryNode>,
    pub edges: Vec<MemoryEdge>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct RecallHit {
    pub source: String,
    pub ref_id: String,
    pub session_id: Option<String>,
    pub snippet: String,
}
