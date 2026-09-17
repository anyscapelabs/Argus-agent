use std::collections::{HashMap, HashSet, VecDeque};

use petgraph::graph::{DiGraph, NodeIndex};
use rusqlite::{params, Connection};
use uuid::Uuid;

use super::schema::{
    NewSessionMemory, SessionMemory, SessionMemoryGraph, SessionMemoryHit, SessionMemoryNode,
};

fn norm_outcome(outcome: &str) -> String {
    match outcome.trim().to_lowercase().as_str() {
        "ok" | "success" | "succeeded" => "ok".into(),
        "partial" => "partial".into(),
        "error" | "failed" | "failure" => "error".into(),
        _ => "unknown".into(),
    }
}

fn norm_kind(kind: &str) -> String {
    let k = kind.trim().to_lowercase();
    if k.is_empty() {
        return "related".into();
    }

    k
}

fn clip(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

pub fn save_session_memory(
    conn: &Connection,
    m: &NewSessionMemory,
) -> Result<SessionMemory, String> {
    let task = m.task.trim().to_string();
    if task.is_empty() {
        return Err("session memory needs a task".into());
    }

    let id = Uuid::new_v4().to_string();
    let importance = m.importance.clamp(1, 5);
    conn.execute(
        "INSERT INTO session_memory (id, session_id, task, summary, outcome, importance) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            id,
            m.session_id,
            clip(&task, 240),
            clip(m.summary.trim(), 2000),
            norm_outcome(&m.outcome),
            importance
        ],
    )
    .map_err(|err| err.to_string())?;

    get_session_memory(conn, &id)
}

pub fn get_session_memory(conn: &Connection, id: &str) -> Result<SessionMemory, String> {
    conn.query_row(
        "SELECT id, session_id, task, summary, outcome, importance, created_at, updated_at \
         FROM session_memory WHERE id = ?1",
        params![id],
        |r| {
            Ok(SessionMemory {
                id: r.get(0)?,
                session_id: r.get(1)?,
                task: r.get(2)?,
                summary: r.get(3)?,
                outcome: r.get(4)?,
                importance: r.get(5)?,
                created_at: r.get(6)?,
                updated_at: r.get(7)?,
            })
        },
    )
    .map_err(|err| err.to_string())
}

pub fn add_session_edge(
    conn: &Connection,
    from_id: &str,
    to_id: &str,
    kind: &str,
) -> Result<bool, String> {
    if from_id.trim().is_empty() || to_id.trim().is_empty() {
        return Err("session memory edge needs both endpoints".into());
    }

    let changed = conn
        .execute(
            "INSERT OR IGNORE INTO session_memory_links (from_id, to_id, kind) VALUES (?1, ?2, ?3)",
            params![from_id.trim(), to_id.trim(), norm_kind(kind)],
        )
        .map_err(|err| err.to_string())?;
    Ok(changed == 1)
}

fn link_pairs(conn: &Connection) -> Vec<(String, String)> {
    let mut out = vec![];
    let Ok(mut stmt) = conn.prepare("SELECT from_id, to_id FROM session_memory_links LIMIT 5000")
    else {
        return out;
    };
    let Ok(rows) = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
    else {
        return out;
    };
    for r in rows.flatten() {
        out.push(r);
    }
    out
}

fn distances(graph: &DiGraph<String, ()>, seeds: &[NodeIndex]) -> HashMap<NodeIndex, u32> {
    let mut dist: HashMap<NodeIndex, u32> = HashMap::new();
    let mut queue = VecDeque::new();
    for s in seeds {
        dist.insert(*s, 0);
        queue.push_back(*s);
    }
    while let Some(n) = queue.pop_front() {
        let d = dist[&n];
        if d >= 2 {
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
    dist
}

pub fn recall_session_memory(
    conn: &Connection,
    query: &str,
    limit: i64,
) -> Result<Vec<SessionMemoryHit>, String> {
    let terms: Vec<String> = query
        .split_whitespace()
        .map(|t| format!("\"{}\"", t.replace('"', "")))
        .collect();
    if terms.is_empty() {
        return Ok(vec![]);
    }
    let q = terms.join(" ");

    let mut stmt = conn
        .prepare(
            "SELECT e.id, e.task, substr(e.summary, 1, 280) FROM session_memory e \
             WHERE e.rowid IN (SELECT rowid FROM session_memory_fts WHERE session_memory_fts MATCH ?1 \
             ORDER BY bm25(session_memory_fts) LIMIT ?2)",
        )
        .map_err(|err| err.to_string())?;
    let seeds: Vec<(String, String, String)> = stmt
        .query_map(params![q, limit.max(1)], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })
        .map_err(|err| err.to_string())?
        .flatten()
        .collect();
    if seeds.is_empty() {
        return Ok(vec![]);
    }

    let mut graph: DiGraph<String, ()> = DiGraph::new();
    let mut idx: HashMap<String, NodeIndex> = HashMap::new();
    for (a, b) in link_pairs(conn) {
        let ai = *idx
            .entry(a.clone())
            .or_insert_with(|| graph.add_node(a.clone()));
        let bi = *idx
            .entry(b.clone())
            .or_insert_with(|| graph.add_node(b.clone()));
        graph.add_edge(ai, bi, ());
    }

    let mut seen: HashSet<String> = HashSet::new();
    let mut out: Vec<SessionMemoryHit> = vec![];
    for (id, task, snippet) in &seeds {
        if seen.insert(id.clone()) {
            out.push(SessionMemoryHit {
                id: id.clone(),
                task: task.clone(),
                snippet: snippet.clone(),
                distance: 0,
            });
        }
    }

    if let Some(first) = seeds.first() {
        if let Some(start) = idx.get(&first.0) {
            let dists = distances(&graph, &[*start]);
            let mut extra: Vec<(u32, String)> = dists
                .into_iter()
                .filter(|(_, d)| *d > 0)
                .filter_map(|(n, d)| {
                    let id = graph.node_weight(n)?.clone();
                    (!seen.contains(&id)).then_some((d, id))
                })
                .collect();
            extra.sort();
            for (d, id) in extra.into_iter().take(limit.max(1) as usize) {
                if seen.insert(id.clone()) {
                    if let Ok(e) = get_session_memory(conn, &id) {
                        out.push(SessionMemoryHit {
                            id,
                            task: e.task.clone(),
                            snippet: clip(&e.summary, 280),
                            distance: d,
                        });
                    }
                }
            }
        }
    }

    out.truncate(limit.max(1) as usize);
    Ok(out)
}

pub fn decay_session_memory(conn: &Connection, older_than_days: i64) -> Result<u64, String> {
    let days = older_than_days.max(1);
    let changed = conn
        .execute(
            "UPDATE session_memory SET importance = importance - 1, updated_at = datetime('now') \
             WHERE created_at < datetime('now', '-' || ?1 || ' days') AND importance > 1",
            params![days],
        )
        .map_err(|err| err.to_string())?;
    Ok(changed as u64)
}

pub fn rollup_session_candidates(
    conn: &Connection,
    limit: i64,
) -> Result<Vec<SessionMemory>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT e.id, e.session_id, e.task, e.summary, e.outcome, e.importance, \
             e.created_at, e.updated_at, COUNT(l.to_id) AS degree \
             FROM session_memory e LEFT JOIN session_memory_links l \
             ON l.from_id = e.id OR l.to_id = e.id \
             WHERE e.importance <= 2 \
             GROUP BY e.id HAVING degree >= 1 \
             ORDER BY degree DESC, e.created_at ASC LIMIT ?1",
        )
        .map_err(|err| err.to_string())?;
    let out: Vec<SessionMemory> = stmt
        .query_map(params![limit.max(1)], |r| {
            Ok(SessionMemory {
                id: r.get(0)?,
                session_id: r.get(1)?,
                task: r.get(2)?,
                summary: r.get(3)?,
                outcome: r.get(4)?,
                importance: r.get(5)?,
                created_at: r.get(6)?,
                updated_at: r.get(7)?,
            })
        })
        .map_err(|err| err.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|err| err.to_string())?;
    Ok(out)
}

pub fn load_session_graph(conn: &Connection, limit: i64) -> Result<SessionMemoryGraph, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, task, importance FROM session_memory \
             ORDER BY created_at DESC LIMIT ?1",
        )
        .map_err(|err| err.to_string())?;
    let ids: Vec<(String, String, i64)> = stmt
        .query_map(params![limit.max(1)], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })
        .map_err(|err| err.to_string())?
        .flatten()
        .collect();
    let keep: HashSet<String> = ids.iter().map(|(id, _, _)| id.clone()).collect();

    let mut degree: HashMap<String, usize> = HashMap::new();
    let mut edges: Vec<(String, String, String)> = vec![];
    let mut estmt = conn
        .prepare("SELECT from_id, to_id, kind FROM session_memory_links LIMIT 2000")
        .map_err(|err| err.to_string())?;
    let rows = estmt
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })
        .map_err(|err| err.to_string())?;
    for r in rows.flatten() {
        if keep.contains(&r.0) && keep.contains(&r.1) {
            *degree.entry(r.0.clone()).or_insert(0) += 1;
            *degree.entry(r.1.clone()).or_insert(0) += 1;
            edges.push(r);
        }
    }

    let mut nodes: Vec<SessionMemoryNode> = ids
        .into_iter()
        .map(|(id, task, importance)| {
            let degree = degree.get(&id).copied().unwrap_or(0);
            SessionMemoryNode {
                id,
                task,
                importance,
                degree,
            }
        })
        .collect();
    nodes.sort_by_key(|n| (std::cmp::Reverse(n.degree), n.id.clone()));
    Ok(SessionMemoryGraph { nodes, edges })
}

pub fn session_memory_from_turn(
    user_text: &str,
    tools_used: &[String],
    ok: bool,
) -> NewSessionMemory {
    let task: String = user_text.lines().next().unwrap_or("").trim().to_string();
    let task = if task.is_empty() {
        "untitled turn".to_string()
    } else {
        clip(&task, 240)
    };
    let mut seen: Vec<String> = vec![];
    for t in tools_used {
        if !seen.contains(t) {
            seen.push(t.clone());
        }
        if seen.len() >= 12 {
            break;
        }
    }
    let summary = if seen.is_empty() {
        "(no tools used)".to_string()
    } else {
        format!("tools: {}", seen.join(", "))
    };
    let importance = if !ok {
        1
    } else if seen.is_empty() {
        1
    } else {
        2
    };
    NewSessionMemory {
        session_id: String::new(),
        task,
        summary,
        outcome: if ok { "ok".into() } else { "error".into() },
        importance,
    }
}
