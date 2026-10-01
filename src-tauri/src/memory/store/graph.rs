// Read-only projections over rows written elsewhere.

use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

use petgraph::graph::{DiGraph, NodeIndex};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use super::memories::{clip, list};
use crate::memory::schema::{MemoryEdge, MemoryGraph, MemoryNode};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Turn {
    pub seq: i64,
    pub who: String,
    pub text: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Transcript {
    pub session_id: String,
    pub title: String,
    pub turns: Vec<Turn>,
    pub next_seq: i64,
    pub more: bool,
}

const TURN_CHARS: usize = 700;
const STRIP_TAGS: &[&str] = &[
    "terminal",
    "browser-action",
    "action",
    "tool-result",
    "document",
    "sandbox",
    "warning",
    "check",
    "thinking",
];

/// Tool output and rendered blocks carry no meaning on replay.
fn strip_blocks(s: &str) -> String {
    static RES: OnceLock<Vec<regex::Regex>> = OnceLock::new();
    let res = RES.get_or_init(|| {
        STRIP_TAGS
            .iter()
            .filter_map(|tag| {
                regex::Regex::new(&format!(r"(?s)<{tag}\b[^>]*>.*?</{tag}>|<{tag}\b[^>]*/>")).ok()
            })
            .collect()
    });

    let mut t = s.to_string();

    for re in res {
        t = re.replace_all(&t, " ").into_owned();
    }

    t.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Read a past session as prose. `after_seq` pages through a long chat.
pub fn read_session(
    conn: &Connection,
    session_id: &str,
    after_seq: i64,
    limit: i64,
) -> Result<Transcript, String> {
    let title: String = conn
        .query_row(
            "SELECT title FROM sessions WHERE id = ?1",
            params![session_id],
            |r| r.get(0),
        )
        .map_err(|_| format!("no session {session_id}"))?;

    let page = limit.clamp(1, 40);
    let cap = page + 1;

    let mut stmt = conn
        .prepare(
            "SELECT seq, role, content FROM messages
             WHERE session_id = ?1 AND active = 1 AND seq > ?2
               AND (role = 'system' OR content NOT LIKE '<tool-result%')
             ORDER BY seq LIMIT ?3",
        )
        .map_err(|err| err.to_string())?;

    let rows: Vec<(i64, String, String)> = stmt
        .query_map(params![session_id, after_seq, cap], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })
        .map_err(|err| err.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|err| err.to_string())?;

    let more = rows.len() > page as usize;
    let mut turns: Vec<Turn> = vec![];

    for (seq, role, content) in rows.into_iter().take(page as usize) {
        let text = strip_blocks(&content);
        if text.is_empty() {
            continue;
        }

        turns.push(Turn {
            seq,
            who: if role == "user" {
                "user".into()
            } else {
                "agent".into()
            },
            text: clip(&text, TURN_CHARS),
        });
    }

    let next_seq = turns.last().map(|t| t.seq).unwrap_or(after_seq);

    Ok(Transcript {
        session_id: session_id.into(),
        title,
        turns,
        next_seq,
        more,
    })
}

pub fn load_graph(conn: &Connection, limit: i64) -> Result<MemoryGraph, String> {
    let mems = list(conn, None, limit.max(50))?;
    let mut graph: DiGraph<String, String> = DiGraph::new();
    let mut idx: HashMap<String, NodeIndex> = HashMap::new();
    let mut nodes: Vec<MemoryNode> = vec![];
    let mut node_ids: HashSet<String> = HashSet::new();
    for m in &mems {
        let id = format!("memory:{}", m.id);
        idx.entry(id.clone())
            .or_insert_with_key(|k| graph.add_node(k.clone()));
        node_ids.insert(id.clone());
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
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
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
