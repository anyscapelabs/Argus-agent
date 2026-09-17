pub mod schema;
pub mod session_memory;
pub mod store;

use tauri::State;

use crate::gateway::schema::{ChatReq, WireMsg};
use crate::gateway::Gateway;
use rusqlite::Connection;
use schema::{Memory, MemoryGraph, MemoryLink, NewMemory, RecallHit};

#[tauri::command]
pub fn memory_save(gw: State<'_, Gateway>, mem: NewMemory) -> Result<Memory, String> {
    let conn = gw.conn.lock().map_err(|err| err.to_string())?;
    store::save(&conn, &mem)
}

#[tauri::command]
pub fn memory_get(gw: State<'_, Gateway>, id: String) -> Result<Memory, String> {
    let conn = gw.conn.lock().map_err(|err| err.to_string())?;
    store::get(&conn, &id)
}

#[tauri::command]
pub fn memory_list(
    gw: State<'_, Gateway>,
    kind: Option<String>,
    limit: Option<i64>,
) -> Result<Vec<Memory>, String> {
    let conn = gw.conn.lock().map_err(|err| err.to_string())?;
    store::list(&conn, kind.as_deref(), limit.unwrap_or(50))
}

#[tauri::command]
pub fn memory_delete(gw: State<'_, Gateway>, id: String) -> Result<(), String> {
    let conn = gw.conn.lock().map_err(|err| err.to_string())?;
    store::delete(&conn, &id)
}

#[tauri::command]
pub fn memory_search(
    gw: State<'_, Gateway>,
    query: String,
    limit: Option<i64>,
) -> Result<Vec<Memory>, String> {
    let conn = gw.conn.lock().map_err(|err| err.to_string())?;
    store::search(&conn, &query, limit.unwrap_or(10))
}

#[tauri::command]
pub fn memory_link(
    gw: State<'_, Gateway>,
    from_id: String,
    to_id: String,
    relation: Option<String>,
) -> Result<MemoryLink, String> {
    let conn = gw.conn.lock().map_err(|err| err.to_string())?;
    store::link(&conn, &from_id, &to_id, &relation.unwrap_or_else(|| "related".into()))
}

#[tauri::command]
pub fn memory_unlink(gw: State<'_, Gateway>, from_id: String, to_id: String) -> Result<(), String> {
    let conn = gw.conn.lock().map_err(|err| err.to_string())?;
    store::unlink(&conn, &from_id, &to_id)
}

#[tauri::command]
pub fn memory_recall(
    gw: State<'_, Gateway>,
    query: String,
    limit: Option<i64>,
) -> Result<Vec<RecallHit>, String> {
    let conn = gw.conn.lock().map_err(|err| err.to_string())?;
    store::recall(&conn, &query, limit.unwrap_or(12))
}

#[tauri::command]
pub fn memory_graph(gw: State<'_, Gateway>, limit: Option<i64>) -> Result<MemoryGraph, String> {
    let conn = gw.conn.lock().map_err(|err| err.to_string())?;
    store::load_graph(&conn, limit.unwrap_or(80))
}

pub fn rollup_prompt(mems: &[Memory]) -> String {
    let mut s = String::from(
        "Condense the following related memories into one durable fact for future sessions. \
         Keep concrete details (names, preferences, decisions); drop redundancies and chatter. \
         Reply with the condensed fact as plain text only, no preamble, under 600 characters.\n",
    );
    for (i, m) in mems.iter().enumerate() {
        s.push_str(&format!("\n{}. [{}] {}\n", i + 1, m.kind, m.content));
    }
    s
}

pub fn apply_rollup(conn: &Connection, candidate_ids: &[String], summary: &str) -> Result<Memory, String> {
    let summary = summary.trim().to_string();
    if summary.is_empty() {
        return Err("rollup needs a summary".into());
    }

    let mut sources: Vec<String> = vec![];
    for id in candidate_ids {
        if !sources.contains(id) && store::get(conn, id).is_ok() {
            sources.push(id.clone());
        }
    }
    if sources.is_empty() {
        return Err("no rollup candidates found".into());
    }

    let condensed = store::save(
        conn,
        &NewMemory {
            content: summary,
            kind: Some("fact".into()),
            importance: Some(3),
            session_id: None,
        },
    )?;
    for id in &sources {
        let _ = store::link(conn, id, &condensed.id, "condensed_into");
    }
    Ok(condensed)
}

pub async fn rollup_with_model(
    gw: &Gateway,
    candidate_ids: Vec<String>,
) -> Result<Memory, String> {
    let model = {
        let conn = gw.conn.lock().map_err(|err| err.to_string())?;
        crate::prompt::compressor::utility_model(&conn)?
    };
    let sources: Vec<Memory> = {
        let conn = gw.conn.lock().map_err(|err| err.to_string())?;
        let mut out = vec![];
        for id in &candidate_ids {
            if let Ok(m) = store::get(&conn, id) {
                out.push(m);
            }
        }
        out
    };
    if sources.is_empty() {
        return Err("no rollup candidates found".into());
    }

    let req = ChatReq {
        model,
        msgs: vec![WireMsg {
            role: "user".into(),
            content: rollup_prompt(&sources),
            ..Default::default()
        }],
        prefix_hash: None,
        tools: vec![],
    };
    let resp = crate::gateway::router::run_opts(gw, &req, 2).await?;
    let conn = gw.conn.lock().map_err(|err| err.to_string())?;
    apply_rollup(&conn, &candidate_ids, &resp.content)
}
