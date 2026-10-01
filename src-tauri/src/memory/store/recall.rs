// FTS over memories, messages, summaries, files; then the rollup into ranked
// past conversations.

use std::collections::{HashMap, HashSet};

use petgraph::graph::{DiGraph, NodeIndex};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use super::memories::{clip, fts_query, get, search};
use crate::memory::schema::RecallHit;

fn search_messages(conn: &Connection, q: &str, limit: i64) -> Vec<RecallHit> {
    let sql = "SELECT m.session_id, m.seq, substr(m.content, 1, 280) FROM messages m WHERE m.rowid IN (SELECT rowid FROM message_fts WHERE message_fts MATCH ?1 ORDER BY bm25(message_fts) LIMIT ?2)";
    let mut out = vec![];
    let Ok(mut stmt) = conn.prepare(sql) else {
        return out;
    };
    let Ok(rows) = stmt.query_map(params![q, limit], |r| {
        Ok((
            r.get::<_, Option<String>>(0)?,
            r.get::<_, i64>(1)?,
            r.get::<_, String>(2)?,
        ))
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
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
        ))
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
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, Option<String>>(1)?,
            r.get::<_, String>(2)?,
        ))
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
    let rows = match stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))) {
        Ok(r) => r,
        Err(_) => return vec![],
    };
    for r in rows.flatten() {
        let a = *idx
            .entry(r.0.clone())
            .or_insert_with_key(|k| graph.add_node(k.clone()));
        let b = *idx
            .entry(r.1.clone())
            .or_insert_with_key(|k| graph.add_node(k.clone()));
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
            dist.entry(m).or_insert_with_key(|&m| {
                queue.push_back(m);
                d + 1
            });
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

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PastSession {
    pub session_id: String,
    pub title: String,
    pub updated_at: String,
    pub score: f64,
    pub snippets: Vec<String>,
}

const SNIPPET_CHARS: usize = 220;
const MAX_SNIPPETS: usize = 3;

const STOP: &[&str] = &[
    "a", "an", "and", "are", "as", "at", "be", "but", "by", "can", "did", "do", "does", "for",
    "from", "had", "has", "have", "how", "i", "if", "in", "is", "it", "its", "me", "my", "of",
    "on", "or", "our", "please", "so", "that", "the", "their", "them", "then", "there", "these",
    "they", "this", "to", "up", "us", "was", "we", "were", "what", "when", "where", "which", "who",
    "why", "will", "with", "you", "your",
];

fn query_terms(query: &str) -> Vec<String> {
    let mut out: Vec<String> = vec![];

    for raw in query.split_whitespace() {
        let t = raw
            .trim_matches(|c: char| !c.is_alphanumeric())
            .to_lowercase();

        if t.len() < 3 || STOP.contains(&t.as_str()) || out.contains(&t) {
            continue;
        }

        out.push(t);
    }

    out
}

/// FTS5 MATCH space is AND, so a whole sentence finds nothing.
fn fts_or_query(terms: &[String]) -> Option<String> {
    if terms.is_empty() {
        return None;
    }

    Some(
        terms
            .iter()
            .map(|t| format!("\"{}\"", t.replace('"', "")))
            .collect::<Vec<_>>()
            .join(" OR "),
    )
}

fn hits_to_past(
    conn: &Connection,
    hits: Vec<RecallHit>,
    exclude: Option<&str>,
) -> Vec<PastSession> {
    let mut order: Vec<String> = vec![];
    let mut by_sid: HashMap<String, Vec<String>> = HashMap::new();
    let mut scores: HashMap<String, f64> = HashMap::new();

    for h in hits {
        let Some(sid) = h.session_id else {
            continue;
        };

        if exclude.is_some_and(|x| x == sid) {
            continue;
        }

        let bucket = by_sid.entry(sid.clone()).or_default();
        if bucket.is_empty() {
            order.push(sid.clone());
        }
        if bucket.len() < MAX_SNIPPETS {
            bucket.push(clip(&h.snippet, SNIPPET_CHARS));
        }

        let s = scores.entry(sid).or_insert(0.0);
        if h.source == "summary" {
            *s += 2.0;
        } else {
            *s += 1.0;
        }
    }

    order
        .into_iter()
        .map(|sid| {
            let title: String = conn
                .query_row(
                    "SELECT title FROM sessions WHERE id = ?1",
                    params![sid],
                    |r| r.get(0),
                )
                .unwrap_or_else(|_| "(untitled)".into());
            let updated_at: String = conn
                .query_row(
                    "SELECT updated_at FROM sessions WHERE id = ?1",
                    params![sid],
                    |r| r.get(0),
                )
                .unwrap_or_default();

            let score = scores.get(&sid).copied().unwrap_or(0.0);
            let snippets = by_sid.remove(&sid).unwrap_or_default();
            PastSession {
                session_id: sid,
                title,
                updated_at,
                score,
                snippets,
            }
        })
        .collect()
}

pub fn recall_sessions(
    conn: &Connection,
    query: &str,
    exclude: Option<&str>,
    limit: i64,
) -> Result<Vec<PastSession>, String> {
    let terms = query_terms(query);
    if terms.is_empty() {
        return Ok(vec![]);
    }

    let cap = limit.clamp(1, 12);
    let and_q = match fts_query(&terms.join(" ")) {
        Some(q) => q,
        None => return Ok(vec![]),
    };

    let mut hits = search_messages(conn, &and_q, cap * 4);
    hits.extend(search_summaries(conn, &and_q, cap * 2));

    if hits.is_empty() {
        if let Some(or_q) = fts_or_query(&terms) {
            hits = search_messages(conn, &or_q, cap * 4);
            hits.extend(search_summaries(conn, &or_q, cap * 2));
        }
    }

    let mut out = hits_to_past(conn, hits, exclude);
    out.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| b.updated_at.cmp(&a.updated_at))
    });
    out.truncate(cap as usize);

    Ok(out)
}
