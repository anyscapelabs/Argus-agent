// Memory rows: save, list, delete, and the keyword autolink that grows
// `memory_links` as memories accumulate. Everything here is one row at a time.

use std::collections::HashSet;

use rusqlite::{params, Connection, OptionalExtension};
use uuid::Uuid;

use crate::memory::schema::{Memory, MemoryLink, NewMemory};

const COLS: &str = "id, content, kind, importance, session_id, created_at, updated_at";

fn row_memory(r: &rusqlite::Row) -> rusqlite::Result<Memory> {
    Ok(Memory {
        id: r.get(0)?,
        content: r.get(1)?,
        kind: r.get(2)?,
        importance: r.get(3)?,
        session_id: r.get(4)?,
        created_at: r.get(5)?,
        updated_at: r.get(6)?,
    })
}

pub fn migrate(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(crate::memory::schema::MIGRATE)
        .map_err(|err| err.to_string())
}

fn norm_kind(kind: Option<String>) -> String {
    let k = kind.unwrap_or_else(|| "fact".into()).trim().to_lowercase();
    match k.as_str() {
        "fact" | "preference" | "project" | "person" | "decision" => k,
        _ => "fact".into(),
    }
}

pub(super) fn clip(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        return s.into();
    }
    s.chars().take(n).collect::<String>()
}

pub(super) fn fts_query(query: &str) -> Option<String> {
    let parts: Vec<String> = query
        .split_whitespace()
        .map(|t| format!("\"{}\"", t.replace('"', "")))
        .collect();
    if parts.is_empty() {
        return None;
    }
    Some(parts.join(" "))
}

pub fn save(conn: &Connection, m: &NewMemory) -> Result<Memory, String> {
    let content = m.content.trim().to_string();
    if content.is_empty() || content.len() > 8192 {
        return Err("memory content must be 1-8192 bytes".into());
    }
    let importance = m.importance.unwrap_or(1).clamp(1, 5);
    let kind = norm_kind(m.kind.clone());
    let id = Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO memories (id, content, kind, importance, session_id) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![id, content, kind, importance, m.session_id],
    )
    .map_err(|err| err.to_string())?;
    let _ = autolink(conn, &id, &content);
    get(conn, &id)
}

const LINK_STOP: &[&str] = &[
    "the", "a", "an", "and", "or", "for", "with", "from", "that", "this", "have", "has", "are",
    "was", "were", "will", "would", "could", "should", "can", "not", "but", "you", "your", "yours",
    "about", "into", "over", "after", "before", "when", "what", "which", "their", "there", "they",
    "them", "then", "than", "also", "just", "like", "user", "users", "using", "used", "does",
    "did", "been", "being", "more", "most", "such", "only", "very", "well",
];

fn keywords(content: &str) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut out: Vec<String> = vec![];
    for w in content.to_lowercase().split(|c: char| !c.is_alphanumeric()) {
        if w.len() < 4 || LINK_STOP.contains(&w) || !seen.insert(w.to_string()) {
            continue;
        }
        out.push(w.to_string());
        if out.len() >= 8 {
            break;
        }
    }
    out
}

pub fn autolink(conn: &Connection, id: &str, content: &str) -> Result<usize, String> {
    if content.chars().count() < 40 {
        return Ok(0);
    }
    let terms = keywords(content);
    if terms.len() < 2 {
        return Ok(0);
    }
    let q = terms
        .iter()
        .map(|t| format!("\"{t}\""))
        .collect::<Vec<_>>()
        .join(" OR ");
    let mut stmt = conn
        .prepare(
            "SELECT id FROM memories WHERE id != ?2 AND rowid IN \
             (SELECT rowid FROM memory_fts WHERE memory_fts MATCH ?1 \
              ORDER BY bm25(memory_fts) LIMIT 3)",
        )
        .map_err(|err| err.to_string())?;
    let ids = stmt
        .query_map(params![q, id], |r| r.get::<_, String>(0))
        .map_err(|err| err.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|err| err.to_string())?;
    let mut n = 0;
    for other in ids {
        link(conn, id, &other, "related")?;
        n += 1;
    }
    Ok(n)
}

pub fn autolink_all(conn: &Connection) -> Result<usize, String> {
    let rows: Vec<(String, String)> = conn
        .prepare("SELECT id, content FROM memories")
        .map_err(|err| err.to_string())?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .map_err(|err| err.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|err| err.to_string())?;
    let mut total = 0;
    for (id, content) in &rows {
        total += autolink(conn, id, content)?;
    }
    Ok(total)
}

pub fn get(conn: &Connection, id: &str) -> Result<Memory, String> {
    conn.query_row(
        &format!("SELECT {COLS} FROM memories WHERE id = ?1"),
        params![id],
        row_memory,
    )
    .optional()
    .map_err(|err| err.to_string())?
    .ok_or_else(|| "memory not found".into())
}

pub fn list(conn: &Connection, kind: Option<&str>, limit: i64) -> Result<Vec<Memory>, String> {
    let sql = match kind {
        Some(_) => format!("SELECT {COLS} FROM memories WHERE kind = ?1 ORDER BY importance DESC, updated_at DESC LIMIT ?2"),
        None => format!("SELECT {COLS} FROM memories ORDER BY importance DESC, updated_at DESC LIMIT ?1"),
    };
    let mut stmt = conn.prepare(&sql).map_err(|err| err.to_string())?;
    let rows = match kind {
        Some(k) => stmt.query_map(params![k, limit], row_memory),
        None => stmt.query_map(params![limit], row_memory),
    }
    .map_err(|err| err.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|err| err.to_string())
}

pub fn delete(conn: &Connection, id: &str) -> Result<(), String> {
    conn.execute("DELETE FROM memories WHERE id = ?1", params![id])
        .map_err(|err| err.to_string())?;
    conn.execute(
        "DELETE FROM memory_links WHERE from_id = ?1 OR to_id = ?1",
        params![id],
    )
    .map_err(|err| err.to_string())?;
    Ok(())
}

pub fn search(conn: &Connection, query: &str, limit: i64) -> Result<Vec<Memory>, String> {
    let q = match fts_query(query) {
        Some(q) => q,
        None => return Ok(vec![]),
    };
    let sql = format!(
        "SELECT {COLS} FROM memories WHERE rowid IN (SELECT rowid FROM memory_fts WHERE memory_fts MATCH ?1 ORDER BY bm25(memory_fts) LIMIT ?2)"
    );
    let mut stmt = conn.prepare(&sql).map_err(|err| err.to_string())?;
    let rows = stmt
        .query_map(params![q, limit], row_memory)
        .map_err(|err| err.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|err| err.to_string())
}

pub fn link(
    conn: &Connection,
    from_id: &str,
    to_id: &str,
    relation: &str,
) -> Result<MemoryLink, String> {
    if from_id.is_empty() || to_id.is_empty() || from_id == to_id {
        return Err("link needs two distinct ids".into());
    }
    let rel = relation.trim().to_string();
    let rel = if rel.is_empty() {
        "related".into()
    } else {
        clip(&rel, 64)
    };
    conn.execute(
        "INSERT INTO memory_links (from_id, to_id, relation) VALUES (?1, ?2, ?3) ON CONFLICT(from_id, to_id) DO UPDATE SET relation = ?3",
        params![from_id, to_id, rel],
    )
    .map_err(|err| err.to_string())?;
    conn.query_row(
        "SELECT from_id, to_id, relation, created_at FROM memory_links WHERE from_id = ?1 AND to_id = ?2",
        params![from_id, to_id],
        |r| {
            Ok(MemoryLink {
                from_id: r.get(0)?,
                to_id: r.get(1)?,
                relation: r.get(2)?,
                created_at: r.get(3)?,
            })
        },
    )
    .map_err(|err| err.to_string())
}

pub fn unlink(conn: &Connection, from_id: &str, to_id: &str) -> Result<(), String> {
    conn.execute(
        "DELETE FROM memory_links WHERE from_id = ?1 AND to_id = ?2",
        params![from_id, to_id],
    )
    .map_err(|err| err.to_string())?;
    Ok(())
}

pub fn index_file(
    conn: &Connection,
    ref_id: &str,
    session_id: Option<&str>,
    content: &str,
) -> Result<(), String> {
    let body = clip(content.trim(), 8000);
    if body.is_empty() {
        return Ok(());
    }
    conn.execute(
        "INSERT INTO file_fts (ref_id, session_id, content) VALUES (?1, ?2, ?3) ON CONFLICT(ref_id) DO UPDATE SET session_id = ?2, content = ?3",
        params![ref_id, session_id, body],
    )
    .map_err(|err| err.to_string())?;
    Ok(())
}
