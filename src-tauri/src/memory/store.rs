use std::collections::{HashMap, HashSet};

use petgraph::graph::{DiGraph, NodeIndex};
use rusqlite::{params, Connection, OptionalExtension};
use uuid::Uuid;

use super::schema::{Memory, MemoryEdge, MemoryGraph, MemoryLink, MemoryNode, NewMemory, RecallHit};

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
    conn.execute_batch(super::schema::MIGRATE)
        .map_err(|err| err.to_string())
}

fn norm_kind(kind: Option<String>) -> String {
    let k = kind.unwrap_or_else(|| "fact".into()).trim().to_lowercase();
    match k.as_str() {
        "fact" | "preference" | "project" | "person" | "decision" => k,
        _ => "fact".into(),
    }
}

fn clip(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        return s.into();
    }
    s.chars().take(n).collect::<String>()
}

fn fts_query(query: &str) -> Option<String> {
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
    get(conn, &id)
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

pub fn link(conn: &Connection, from_id: &str, to_id: &str, relation: &str) -> Result<MemoryLink, String> {
    if from_id.is_empty() || to_id.is_empty() || from_id == to_id {
        return Err("link needs two distinct ids".into());
    }
    let rel = relation.trim().to_string();
    let rel = if rel.is_empty() { "related".into() } else { clip(&rel, 64) };
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

pub fn index_file(conn: &Connection, ref_id: &str, session_id: Option<&str>, content: &str) -> Result<(), String> {
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

fn search_messages(conn: &Connection, q: &str, limit: i64) -> Vec<RecallHit> {
    let sql = "SELECT m.session_id, m.seq, substr(m.content, 1, 280) FROM messages m WHERE m.rowid IN (SELECT rowid FROM message_fts WHERE message_fts MATCH ?1 ORDER BY bm25(message_fts) LIMIT ?2)";
    let mut out = vec![];
    let Ok(mut stmt) = conn.prepare(sql) else {
        return out;
    };
    let Ok(rows) = stmt.query_map(params![q, limit], |r| {
        Ok((r.get::<_, Option<String>>(0)?, r.get::<_, i64>(1)?, r.get::<_, String>(2)?))
    }) else {
        return out;
    };
    for r in rows.flatten() {
        out.push(RecallHit {
            source: "message".into(),
            ref_id: format!("{}#{}", r.0.clone().unwrap_or_default(), r.1),
            session_id: r.0,
            snippet: r.2,
        });
    }
    out
}

fn search_summaries(conn: &Connection, q: &str, limit: i64) -> Vec<RecallHit> {
    let sql = "SELECT s.session_id, s.id, substr(s.content, 1, 280) FROM summaries s WHERE s.rowid IN (SELECT rowid FROM summary_fts WHERE summary_fts MATCH ?1 ORDER BY bm25(summary_fts) LIMIT ?2)";
    let mut out = vec![];
    let Ok(mut stmt) = conn.prepare(sql) else {
        return out;
    };
    let Ok(rows) = stmt.query_map(params![q, limit], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?))
    }) else {
        return out;
    };
    for r in rows.flatten() {
        out.push(RecallHit {
            source: "summary".into(),
            ref_id: r.1,
            session_id: Some(r.0),
            snippet: r.2,
        });
    }
    out
}

fn search_files(conn: &Connection, q: &str, limit: i64) -> Vec<RecallHit> {
    let sql = "SELECT f.ref_id, f.session_id, substr(f.content, 1, 280) FROM file_fts f WHERE f.rowid IN (SELECT rowid FROM file_fts_idx WHERE file_fts_idx MATCH ?1 ORDER BY bm25(file_fts_idx) LIMIT ?2)";
    let mut out = vec![];
    let Ok(mut stmt) = conn.prepare(sql) else {
        return out;
    };
    let Ok(rows) = stmt.query_map(params![q, limit], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?, r.get::<_, String>(2)?))
    }) else {
        return out;
    };
    for r in rows.flatten() {
        out.push(RecallHit {
            source: "file".into(),
            ref_id: r.0,
            session_id: r.1,
            snippet: r.2,
        });
    }
    out
}

fn expand_seeds(
    conn: &Connection,
    seeds: &HashSet<String>,
    depth: u32,
    cap: usize,
) -> Vec<(u32, String)> {
    let mut graph: DiGraph<String, ()> = DiGraph::new();
    let mut idx: HashMap<String, NodeIndex> = HashMap::new();
    let mut stmt = match conn.prepare("SELECT from_id, to_id FROM memory_links LIMIT 5000") {
        Ok(s) => s,
        Err(_) => return vec![],
    };
    let rows = match stmt.query_map([], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
    }) {
        Ok(r) => r,
        Err(_) => return vec![],
    };
    for r in rows.flatten() {
        let a = *idx
            .entry(r.0.clone())
            .or_insert_with(|| graph.add_node(r.0.clone()));
        let b = *idx
            .entry(r.1.clone())
            .or_insert_with(|| graph.add_node(r.1.clone()));
        graph.add_edge(a, b, ());
    }

    let mut dist: HashMap<NodeIndex, u32> = HashMap::new();
    let mut queue = std::collections::VecDeque::new();
    for s in seeds {
        if let Some(n) = idx.get(s) {
            dist.insert(*n, 0);
            queue.push_back(*n);
        }
    }
    while let Some(n) = queue.pop_front() {
        let d = dist[&n];
        if d >= depth {
            continue;
        }
        for m in graph
            .neighbors_directed(n, petgraph::Direction::Outgoing)
            .chain(graph.neighbors_directed(n, petgraph::Direction::Incoming))
        {
            if !dist.contains_key(&m) {
                dist.insert(m, d + 1);
                queue.push_back(m);
            }
        }
    }

    let mut out: Vec<(u32, String)> = dist
        .into_iter()
        .filter_map(|(n, d)| {
            let id = graph.node_weight(n)?.clone();
            if d == 0 || seeds.contains(&id) {
                return None;
            }
            Some((d, id))
        })
        .collect();
    out.sort();
    out.truncate(cap);
    out
}

pub fn recall(conn: &Connection, query: &str, limit: i64) -> Result<Vec<RecallHit>, String> {
    let q = match fts_query(query) {
        Some(q) => q,
        None => return Ok(vec![]),
    };
    let per = (limit.max(4) / 4).max(2);
    let mems = search(conn, query, per)?;
    let mut hits: Vec<RecallHit> = mems
        .iter()
        .map(|m| RecallHit {
            source: "memory".into(),
            ref_id: m.id.clone(),
            session_id: m.session_id.clone(),
            snippet: clip(&m.content, 280),
        })
        .collect();
    hits.extend(search_messages(conn, &q, per));
    hits.extend(search_summaries(conn, &q, per));
    hits.extend(search_files(conn, &q, per));
    let seeds: HashSet<String> = hits.iter().map(|h| h.ref_id.clone()).collect();
    for (_d, id) in expand_seeds(conn, &seeds, 2, per as usize) {
        if let Ok(m) = get(conn, &id) {
            hits.push(RecallHit {
                source: "memory".into(),
                ref_id: m.id.clone(),
                session_id: m.session_id.clone(),
                snippet: clip(&m.content, 280),
            });
        }
    }
    hits.truncate(limit.max(1) as usize);
    Ok(hits)
}

pub fn load_graph(conn: &Connection, limit: i64) -> Result<MemoryGraph, String> {
    let mems = list(conn, None, limit.max(50))?;
    let mut graph: DiGraph<String, String> = DiGraph::new();
    let mut idx: HashMap<String, NodeIndex> = HashMap::new();
    let mut nodes: Vec<MemoryNode> = vec![];
    let mut node_ids: HashSet<String> = HashSet::new();
    for m in &mems {
        let id = format!("memory:{}", m.id);
        node_ids.insert(id.clone());
        let ni = graph.add_node(id.clone());
        idx.insert(id.clone(), ni);
        nodes.push(MemoryNode {
            id,
            label: clip(&m.content, 72),
            kind: m.kind.clone(),
        });
        if let Some(sid) = &m.session_id {
            let skey = format!("session:{sid}");
            if node_ids.insert(skey.clone()) {
                let ni = graph.add_node(skey.clone());
                idx.insert(skey.clone(), ni);
                nodes.push(MemoryNode {
                    id: skey,
                    label: clip(sid, 12),
                    kind: "session".into(),
                });
            }
        }
    }
    let mut stmt = conn
        .prepare("SELECT from_id, to_id, relation FROM memory_links LIMIT 500")
        .map_err(|err| err.to_string())?;
    let rows = stmt
        .query_map([], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?))
        })
        .map_err(|err| err.to_string())?;
    let mut edges: Vec<MemoryEdge> = vec![];
    let mut seen: HashSet<(String, String)> = HashSet::new();
    for r in rows.flatten() {
        let a = format!("memory:{}", r.0);
        let b = format!("memory:{}", r.1);
        if !node_ids.contains(&a) || !node_ids.contains(&b) {
            continue;
        }
        if let (Some(ai), Some(bi)) = (idx.get(&a), idx.get(&b)) {
            graph.add_edge(*ai, *bi, r.2.clone());
        }
        if seen.insert((a.clone(), b.clone())) {
            edges.push(MemoryEdge {
                from_id: a,
                to_id: b,
                relation: r.2,
            });
        }
    }
    let mut order: Vec<NodeIndex> = graph.node_indices().collect();
    order.sort_by_key(|i| graph.neighbors_undirected(*i).count());
    order.reverse();
    let keep: HashSet<NodeIndex> = order.into_iter().take(120).collect();
    nodes.retain(|n| idx.get(&n.id).is_some_and(|i| keep.contains(i)));
    let kept: HashSet<String> = nodes.iter().map(|n| n.id.clone()).collect();
    edges.retain(|e| kept.contains(&e.from_id) && kept.contains(&e.to_id));
    Ok(MemoryGraph { nodes, edges })
}
