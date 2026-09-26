use std::sync::Arc;

use rusqlite::params;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::Notify;

use crate::gateway::Gateway;
use crate::sessions::chat;
use crate::sessions::store;

/// How many a parent may have in the air at once. Past this it is waiting on
/// more than it can read, and the fan-out stops being parallelism and starts
/// being noise.
pub const MAX_CHILDREN: usize = 4;

/// What the parent is handed back. The full exchange stays in the child
/// session; a summary that cost 40k tokens to produce is not a summary.
pub const SUMMARY_MAX: usize = 1_500;

const NAME_MAX: usize = 40;
const TITLE_MAX: usize = 90;

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct AgentRun {
    pub id: String,
    pub parent_id: String,
    pub name: String,
    pub title: String,
    pub state: String,
    pub result: Option<String>,
    pub created_at: String,
}

fn clip(s: &str, n: usize) -> String {
    let t = s.trim().replace(['\n', '\r'], " ");
    t.chars().take(n).collect()
}

fn run_from(r: &rusqlite::Row<'_>) -> rusqlite::Result<AgentRun> {
    Ok(AgentRun {
        id: r.get(0)?,
        parent_id: r.get(1)?,
        name: r.get::<_, Option<String>>(2)?.unwrap_or_default(),
        title: r.get(3)?,
        state: r
            .get::<_, Option<String>>(4)?
            .unwrap_or_else(|| "running".into()),
        result: r.get(5)?,
        created_at: r.get(6)?,
    })
}

const COLS: &str = "s.id, s.parent_id, s.agent_name, s.title, s.agent_state, \
                    (SELECT content FROM messages m WHERE m.session_id = s.id
                       AND m.role = 'assistant' AND m.active = 1
                       AND m.content NOT LIKE '<tool-result%'
                     ORDER BY m.seq DESC LIMIT 1), s.created_at";

pub fn list(conn: &rusqlite::Connection, parent_id: &str) -> Result<Vec<AgentRun>, String> {
    let mut stmt = conn
        .prepare(&format!(
            "SELECT {COLS} FROM sessions s WHERE s.parent_id = ?1 ORDER BY s.created_at"
        ))
        .map_err(|e| e.to_string())?;

    let rows = stmt
        .query_map(params![parent_id], run_from)
        .map_err(|e| e.to_string())?;

    Ok(rows.flatten().collect())
}

pub fn get(conn: &rusqlite::Connection, id: &str) -> Result<AgentRun, String> {
    conn.query_row(
        &format!("SELECT {COLS} FROM sessions s WHERE s.id = ?1 AND s.parent_id IS NOT NULL"),
        params![id],
        run_from,
    )
    .map_err(|_| format!("no sub-agent {id}"))
}

/// The tail of a child's answer, which is the part a caller actually needs when
/// the head was a summary. The head is what gets cut, not the tail.
pub fn tail(run: &AgentRun, max: usize) -> Result<String, String> {
    let full = run
        .result
        .clone()
        .ok_or_else(|| format!("sub-agent {} has not produced an answer yet", run.id))?;

    let n = full.chars().count();

    Ok(if n > max {
        full.chars().skip(n - max).collect()
    } else {
        full
    })
}

fn running_count(conn: &rusqlite::Connection, parent_id: &str) -> Result<i64, String> {
    conn.query_row(
        "SELECT COUNT(*) FROM sessions WHERE parent_id = ?1 AND agent_state = 'running'",
        params![parent_id],
        |r| r.get(0),
    )
    .map_err(|e| e.to_string())
}

pub struct Spec {
    pub parent_id: String,
    pub name: String,
    pub title: String,
    pub prompt: String,
    pub model_id: Option<String>,
    pub permission: String,
}

/// Create the child, start its turn, detach. The caller gets the card back the
/// moment the row exists and never waits on the work.
pub fn spawn<R: tauri::Runtime>(
    app: &AppHandle<R>,
    gw: &Gateway,
    spec: Spec,
) -> Result<AgentRun, String> {
    if spec.prompt.trim().is_empty() {
        return Err("a sub-agent needs a prompt to work from".into());
    }

    let name = clip(&spec.name, NAME_MAX);
    let title = clip(&spec.title, TITLE_MAX);

    if name.is_empty() {
        return Err("a sub-agent needs a name".into());
    }

    let (child_id, live) = {
        let conn = gw.conn.lock().map_err(|e| e.to_string())?;

        if store::is_child(&conn, &spec.parent_id)? {
            return Err("a sub-agent cannot start another sub-agent".into());
        }

        if running_count(&conn, &spec.parent_id)? >= MAX_CHILDREN as i64 {
            return Err(format!(
                "{MAX_CHILDREN} sub-agents are already running — wait for one to finish"
            ));
        }

        let child = store::create_child(
            &conn,
            &spec.parent_id,
            &name,
            &title,
            spec.model_id.as_deref(),
            &spec.permission,
        )?;

        (child.id, store::get_session(&conn, &spec.parent_id)?)
    };

    let cancel = Arc::new(Notify::new());
    let notify = cancel.clone();

    if let Ok(mut tasks) = gw.tasks.lock() {
        tasks.insert(child_id.clone(), notify);
    }

    let app2 = app.clone();
    let prompt = spec.prompt.clone();
    let parent = spec.parent_id.clone();
    let cid = child_id.clone();
    let nm = name.clone();
    let ti = title.clone();

    tauri::async_runtime::spawn(async move {
        supervise(&app2, &cid, &parent, &nm, &ti, &prompt, &live, cancel).await;
    });

    let conn = gw.conn.lock().map_err(|e| e.to_string())?;
    get(&conn, &child_id)
}

async fn supervise<R: tauri::Runtime>(
    app: &AppHandle<R>,
    child_id: &str,
    parent_id: &str,
    name: &str,
    title: &str,
    prompt: &str,
    parent: &crate::sessions::schema::Session,
    cancel: Arc<Notify>,
) {
    let gw = app.state::<Gateway>();

    let mut owned = parent.clone();
    owned.id = child_id.to_string();

    let sink = chat::FanSink {
        gw: gw.inner(),
        child_id: child_id.to_string(),
        parent_id: parent_id.to_string(),
    };

    // The child does not wait for a slot: a parent that fans out must not
    // block on the first one before starting the second.
    let outcome = crate::tools::shell::CANCEL
        .scope(
            cancel,
            crate::tools::notepad::SESSION_ID.scope(
                Some(child_id.to_string()),
                chat::send(gw.inner(), app, child_id, prompt, &sink, "system"),
            ),
        )
        .await;

    if let Ok(mut tasks) = gw.tasks.lock() {
        tasks.remove(child_id);
    }

    let answer = last_assistant(gw.inner(), child_id);
    let state = if outcome.is_ok() { "done" } else { "failed" };
    let verdict = match outcome {
        Ok(()) => "finished",
        Err(ref e) if e == "stopped" => "was stopped",
        Err(_) => "did not finish",
    };

    if let Ok(conn) = gw.conn.lock() {
        let _ = store::set_agent_state(&conn, child_id, state, None);
    }

    let summary = match &answer {
        Some(a) => clip(a, SUMMARY_MAX),
        None => match outcome {
            Ok(()) => "It finished without saying anything.".to_string(),
            Err(ref e) => format!("It {verdict}: {e}"),
        },
    };

    let body = format!(
        "<agent-done id=\"{child_id}\" name=\"{}\" state=\"{state}\">\n\
         The sub-agent you called {name} {verdict}. Its answer follows; its full \
         transcript is in that sub-agent's own chat, open it from the card to read \
         the whole exchange.\n\n\
         {summary}\n\
         </agent-done>",
        chat::attr_escape(title)
    );

    chat::announce(app, gw.inner(), parent_id, &body, true);

    let _ = app.emit(
        "agent-done",
        AgentDone {
            id: child_id.to_string(),
            name: name.to_string(),
            title: title.to_string(),
            state: state.to_string(),
        },
    );
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AgentDone {
    pub id: String,
    pub name: String,
    pub title: String,
    pub state: String,
}

fn last_assistant(gw: &Gateway, session_id: &str) -> Option<String> {
    let conn = gw.conn.lock().ok()?;
    conn.query_row(
        "SELECT content FROM messages WHERE session_id = ?1 AND role = 'assistant'
           AND active = 1 AND content NOT LIKE '<tool-result%'
         ORDER BY seq DESC LIMIT 1",
        params![session_id],
        |r| r.get::<_, String>(0),
    )
    .ok()
}

pub fn kill(gw: &Gateway, id: &str) -> Result<bool, String> {
    let conn = gw.conn.lock().map_err(|e| e.to_string())?;
    get(&conn, id)?;

    drop(conn);

    let handle = gw.tasks.lock().map_err(|e| e.to_string())?.get(id).cloned();

    match handle {
        Some(n) => {
            n.notify_one();
            Ok(true)
        }
        None => Ok(false),
    }
}

/// Children that were mid-flight when the app died.
pub fn reconcile(conn: &rusqlite::Connection) -> Result<usize, String> {
    conn.execute(
        "UPDATE sessions SET agent_state = 'interrupted' WHERE agent_state = 'running'",
        [],
    )
    .map_err(|e| e.to_string())
}

/// How many finished children a conversation keeps. Their transcripts are
/// read back through the card, so this is the depth of the paper trail.
pub const KEEP_PER_PARENT: usize = 200;

/// Drop the oldest finished children of any conversation that has more than
/// `keep` of them. Running children are never touched, and the cap is counted
/// per parent — a busy conversation never prunes a quiet one's history.
pub fn cleanup(conn: &rusqlite::Connection, keep: usize) -> Result<usize, String> {
    conn.execute(
        "DELETE FROM sessions WHERE id IN (
             SELECT id FROM (
                 SELECT id, ROW_NUMBER() OVER (
                     PARTITION BY parent_id ORDER BY created_at DESC, id DESC) AS rn
                 FROM sessions
                 WHERE parent_id IS NOT NULL AND agent_state <> 'running'
             ) WHERE rn > ?1)",
        params![keep as i64],
    )
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn agent_list(
    gw: tauri::State<'_, Gateway>,
    parent_id: String,
) -> Result<Vec<AgentRun>, String> {
    let conn = gw.conn.lock().map_err(|e| e.to_string())?;
    list(&conn, &parent_id)
}

#[tauri::command]
pub fn agent_read(
    gw: tauri::State<'_, Gateway>,
    id: String,
    max_chars: Option<i64>,
) -> Result<AgentRead, String> {
    let conn = gw.conn.lock().map_err(|e| e.to_string())?;
    let run = get(&conn, &id)?;
    let text = tail(
        &run,
        max_chars.unwrap_or(8_000).clamp(200, 400_000) as usize,
    )?;

    Ok(AgentRead { run, text })
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct AgentRead {
    pub run: AgentRun,
    pub text: String,
}

#[tauri::command]
pub fn agent_kill(gw: tauri::State<'_, Gateway>, id: String) -> Result<bool, String> {
    kill(&gw, &id)
}

pub const PROMPT_SECTION: &str = "PARALLEL WORK\n\
When a task splits into independent pieces, hand each to a sub-agent instead of \
doing them one after another. agent.spawn starts one with a name, a short title, and a \
prompt that stands on its own — it cannot see this conversation, so spell out \
everything it needs and exactly what to hand back.\n\
agent.spawn returns at once. Start every piece before waiting on any of them. \
There are at most four at a time and they cannot start sub-agents of their own.\n\
A sub-agent's answer comes back to you as a summary; the full exchange is in its own \
chat. Read the transcript with agent.read when the summary is not enough — do not \
ask it to repeat work you have not seen.\n\
Ask a sub-agent for a specific deliverable, not a topic. \"Find every caller of \
parse_profile and list them with line numbers\" is a task. \"Look into the parser\" \
is a wish.\n\
A sub-agent that needs approval raises it in this chat, so you will see it. It \
refuses the step rather than waiting when this chat is closed, so if it comes back \
with a refusal, the work did not happen.\n";

pub const CHILD_SECTION: &str = "YOU ARE A SUB-AGENT\n\
You are working inside another agent's chat. Your output is the whole of what they \
receive, so end with the answer and nothing else — no preamble, no offer to help \
further, no description of what you are about to do.\n\
You cannot start sub-agents and you cannot talk to the user. If something needs a \
decision that is not yours, say so in your answer and let your caller decide.\n";
