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

DROP TRIGGER IF EXISTS memory_fts_ai;
DROP TRIGGER IF EXISTS memory_fts_ad;
DROP TRIGGER IF EXISTS memory_fts_au;
DROP TABLE IF EXISTS memory_fts;
CREATE VIRTUAL TABLE memory_fts USING fts5(
  content, kind
);

CREATE TRIGGER memory_fts_ai AFTER INSERT ON memories BEGIN
  INSERT INTO memory_fts(rowid, content, kind)
  VALUES (new.rowid, new.content, new.kind);
END;

DROP TRIGGER IF EXISTS memory_fts_ad;
CREATE TRIGGER memory_fts_ad AFTER DELETE ON memories BEGIN
  DELETE FROM memory_fts WHERE rowid = old.rowid;
END;

DROP TRIGGER IF EXISTS memory_fts_au;
CREATE TRIGGER memory_fts_au AFTER UPDATE ON memories BEGIN
  DELETE FROM memory_fts WHERE rowid = old.rowid;
  INSERT INTO memory_fts(rowid, content, kind)
  VALUES (new.rowid, new.content, new.kind);
END;

INSERT INTO memory_fts(rowid, content, kind)
  SELECT rowid, content, kind FROM memories
  WHERE rowid NOT IN (SELECT rowid FROM memory_fts);

CREATE TABLE IF NOT EXISTS memory_links (
  from_id    TEXT NOT NULL,
  to_id      TEXT NOT NULL,
  relation   TEXT NOT NULL DEFAULT 'related',
  created_at TEXT NOT NULL DEFAULT (datetime('now')),
  PRIMARY KEY (from_id, to_id)
);
CREATE INDEX IF NOT EXISTS idx_memory_links_from ON memory_links(from_id);
CREATE INDEX IF NOT EXISTS idx_memory_links_to ON memory_links(to_id);

CREATE TABLE IF NOT EXISTS session_memory (
  id          TEXT PRIMARY KEY,
  session_id  TEXT NOT NULL,
  task        TEXT NOT NULL,
  summary     TEXT NOT NULL DEFAULT '',
  outcome     TEXT NOT NULL DEFAULT 'unknown',
  importance  INTEGER NOT NULL DEFAULT 1,
  created_at  TEXT NOT NULL DEFAULT (datetime('now')),
  updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_session_memory_session ON session_memory(session_id);
CREATE INDEX IF NOT EXISTS idx_session_memory_importance ON session_memory(importance);

CREATE VIRTUAL TABLE IF NOT EXISTS session_memory_fts USING fts5(
  task, summary
);

CREATE TRIGGER IF NOT EXISTS session_memory_fts_ai AFTER INSERT ON session_memory BEGIN
  INSERT INTO session_memory_fts(rowid, task, summary)
  VALUES (new.rowid, new.task, new.summary);
END;

CREATE TRIGGER IF NOT EXISTS session_memory_fts_ad AFTER DELETE ON session_memory BEGIN
  DELETE FROM session_memory_fts WHERE rowid = old.rowid;
END;

CREATE TRIGGER IF NOT EXISTS session_memory_fts_au AFTER UPDATE ON session_memory BEGIN
  DELETE FROM session_memory_fts WHERE rowid = old.rowid;
  INSERT INTO session_memory_fts(rowid, task, summary)
  VALUES (new.rowid, new.task, new.summary);
END;

CREATE TABLE IF NOT EXISTS session_memory_links (
  from_id    TEXT NOT NULL,
  to_id      TEXT NOT NULL,
  kind       TEXT NOT NULL DEFAULT 'related',
  created_at TEXT NOT NULL DEFAULT (datetime('now')),
  PRIMARY KEY (from_id, to_id, kind)
);
CREATE INDEX IF NOT EXISTS idx_session_memory_links_from ON session_memory_links(from_id);
CREATE INDEX IF NOT EXISTS idx_session_memory_links_to ON session_memory_links(to_id);

CREATE VIRTUAL TABLE IF NOT EXISTS message_fts USING fts5(
  content, content_rowid='rowid'
);

CREATE TRIGGER IF NOT EXISTS message_fts_ai AFTER INSERT ON messages BEGIN
  INSERT INTO message_fts(rowid, content)
  VALUES (new.rowid, new.content);
END;

DROP TRIGGER IF EXISTS message_fts_ad;
CREATE TRIGGER message_fts_ad AFTER DELETE ON messages BEGIN
  DELETE FROM message_fts WHERE rowid = old.rowid;
END;

DROP TRIGGER IF EXISTS message_fts_au;
CREATE TRIGGER message_fts_au AFTER UPDATE ON messages BEGIN
  DELETE FROM message_fts WHERE rowid = old.rowid;
  INSERT INTO message_fts(rowid, content)
  VALUES (new.rowid, new.content);
END;

INSERT INTO message_fts(rowid, content)
  SELECT rowid, content FROM messages
  WHERE rowid NOT IN (SELECT rowid FROM message_fts);

CREATE VIRTUAL TABLE IF NOT EXISTS summary_fts USING fts5(
  content, content_rowid='rowid'
);

CREATE TRIGGER IF NOT EXISTS summary_fts_ai AFTER INSERT ON summaries BEGIN
  INSERT INTO summary_fts(rowid, content)
  VALUES (new.rowid, new.content);
END;

DROP TRIGGER IF EXISTS summary_fts_ad;
CREATE TRIGGER summary_fts_ad AFTER DELETE ON summaries BEGIN
  DELETE FROM summary_fts WHERE rowid = old.rowid;
END;

DROP TRIGGER IF EXISTS summary_fts_au;
CREATE TRIGGER summary_fts_au AFTER UPDATE ON summaries BEGIN
  DELETE FROM summary_fts WHERE rowid = old.rowid;
  INSERT INTO summary_fts(rowid, content)
  VALUES (new.rowid, new.content);
END;

INSERT INTO summary_fts(rowid, content)
  SELECT rowid, content FROM summaries
  WHERE rowid NOT IN (SELECT rowid FROM summary_fts);

CREATE TABLE IF NOT EXISTS file_fts (
  ref_id     TEXT PRIMARY KEY,
  session_id TEXT,
  content    TEXT NOT NULL
);
DROP TRIGGER IF EXISTS file_fts_idx_ai;
DROP TRIGGER IF EXISTS file_fts_idx_ad;
DROP TRIGGER IF EXISTS file_fts_idx_au;
DROP TABLE IF EXISTS file_fts_idx;
CREATE VIRTUAL TABLE file_fts_idx USING fts5(
  content
);
CREATE TRIGGER file_fts_idx_ai AFTER INSERT ON file_fts BEGIN
  INSERT INTO file_fts_idx(rowid, content)
  VALUES (new.rowid, new.content);
END;
DROP TRIGGER IF EXISTS file_fts_idx_ad;
CREATE TRIGGER file_fts_idx_ad AFTER DELETE ON file_fts BEGIN
  DELETE FROM file_fts_idx WHERE rowid = old.rowid;
END;
DROP TRIGGER IF EXISTS file_fts_idx_au;
CREATE TRIGGER file_fts_idx_au AFTER UPDATE ON file_fts BEGIN
  DELETE FROM file_fts_idx WHERE rowid = old.rowid;
  INSERT INTO file_fts_idx(rowid, content)
  VALUES (new.rowid, new.content);
END;

INSERT INTO file_fts_idx(rowid, content)
  SELECT rowid, content FROM file_fts
  WHERE rowid NOT IN (SELECT rowid FROM file_fts_idx);
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

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SessionMemory {
    pub id: String,
    pub session_id: String,
    pub task: String,
    pub summary: String,
    pub outcome: String,
    pub importance: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Default)]
pub struct NewSessionMemory {
    pub session_id: String,
    pub task: String,
    pub summary: String,
    pub outcome: String,
    pub importance: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SessionMemoryHit {
    pub id: String,
    pub task: String,
    pub snippet: String,
    pub distance: u32,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SessionMemoryNode {
    pub id: String,
    pub task: String,
    pub importance: i64,
    pub degree: usize,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SessionMemoryGraph {
    pub nodes: Vec<SessionMemoryNode>,
    pub edges: Vec<(String, String, String)>,
}
