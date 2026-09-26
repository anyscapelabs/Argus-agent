pub mod schema;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use rusqlite::params;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::Notify;

use crate::gateway::schema::StreamEvent;
use crate::gateway::Gateway;
use crate::tools::sandbox;

pub const DEADMAN_MAX: u64 = 12 * 60 * 60;

const TAIL_MAX: usize = 8_000;
const WAKE_TAIL: usize = 1_500;

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Job {
    pub id: String,
    pub session_id: Option<String>,
    pub label: String,
    pub command: String,
    pub cwd: Option<String>,
    pub profile: String,
    pub privilege: String,
    pub state: String,
    pub exit: Option<i64>,
    pub wake: bool,
    pub permission: String,
    pub started_ms: i64,
    pub ended_ms: Option<i64>,
    pub duration_ms: Option<i64>,
    pub out_bytes: i64,
    pub truncated: bool,
    pub note: Option<String>,
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn row_to_job(r: &rusqlite::Row<'_>) -> rusqlite::Result<Job> {
    Ok(Job {
        id: r.get(0)?,
        session_id: r.get(1)?,
        label: r.get(2)?,
        command: r.get(3)?,
        cwd: r.get(4)?,
        profile: r.get(5)?,
        privilege: r.get(6)?,
        state: r.get(7)?,
        exit: r.get(8)?,
        wake: r.get::<_, i64>(9)? != 0,
        permission: r.get(10)?,
        started_ms: r.get(11)?,
        ended_ms: r.get(12)?,
        duration_ms: r.get(13)?,
        out_bytes: r.get(14)?,
        truncated: r.get::<_, i64>(15)? != 0,
        note: r.get(16)?,
    })
}

const COLS: &str = "id, session_id, label, command, cwd, profile, privilege, state, \
                    exit, wake, permission, started_ms, ended_ms, duration_ms, \
                    out_bytes, truncated, note";

pub fn list(
    conn: &rusqlite::Connection,
    session_id: Option<&str>,
    limit: i64,
) -> Result<Vec<Job>, String> {
    let lim = limit.clamp(1, 100);
    let mut out = vec![];

    match session_id {
        Some(sid) => {
            let mut stmt = conn
                .prepare(&format!(
                    "SELECT {COLS} FROM jobs WHERE session_id = ?1 ORDER BY started_ms DESC, rowid DESC LIMIT ?2"
                ))
                .map_err(|e| e.to_string())?;

            let rows = stmt
                .query_map(params![sid, lim], row_to_job)
                .map_err(|e| e.to_string())?;

            for j in rows.flatten() {
                out.push(j);
            }
        }
        None => {
            let mut stmt = conn
                .prepare(&format!(
                    "SELECT {COLS} FROM jobs ORDER BY started_ms DESC, rowid DESC LIMIT ?1"
                ))
                .map_err(|e| e.to_string())?;

            let rows = stmt
                .query_map(params![lim], row_to_job)
                .map_err(|e| e.to_string())?;

            for j in rows.flatten() {
                out.push(j);
            }
        }
    }

    Ok(out)
}

pub fn get(conn: &rusqlite::Connection, id: &str) -> Result<Job, String> {
    conn.query_row(
        &format!("SELECT {COLS} FROM jobs WHERE id = ?1"),
        params![id],
        row_to_job,
    )
    .map_err(|_| format!("no job {id}"))
}

fn set_state(
    conn: &rusqlite::Connection,
    id: &str,
    state: &str,
    exit: Option<i64>,
    duration_ms: Option<i64>,
    out_bytes: i64,
    truncated: bool,
    note: Option<&str>,
) -> Result<(), String> {
    conn.execute(
        "UPDATE jobs SET state = ?2, exit = ?3, ended_ms = ?4, duration_ms = ?5,
                         out_bytes = ?6, truncated = ?7, note = ?8
         WHERE id = ?1",
        params![
            id,
            state,
            exit,
            now_ms(),
            duration_ms,
            out_bytes,
            truncated as i64,
            note
        ],
    )
    .map_err(|e| e.to_string())?;

    Ok(())
}

pub fn reconcile(conn: &rusqlite::Connection) -> Result<usize, String> {
    conn.execute(
        "UPDATE jobs SET state = 'interrupted', ended_ms = ?1,
                         note = 'Argus exited while this was running'
         WHERE state = 'running'",
        params![now_ms()],
    )
    .map_err(|e| e.to_string())
}

pub fn log_path(gw: &Gateway, id: &str) -> PathBuf {
    gw.jobs_dir.join(format!("{id}.log"))
}

pub fn tail(path: &PathBuf, max: usize) -> Result<String, String> {
    let Ok(meta) = std::fs::metadata(path) else {
        return Ok(String::new());
    };

    let len = meta.len() as usize;

    if len == 0 {
        return Ok(String::new());
    }

    let start = len.saturating_sub(max);
    let mut buf = vec![0u8; len - start];
    let mut f = std::fs::File::open(path).map_err(|e| e.to_string())?;

    use std::io::{Read, Seek, SeekFrom};
    f.seek(SeekFrom::Start(start as u64))
        .map_err(|e| e.to_string())?;
    f.read_exact(&mut buf).map_err(|e| e.to_string())?;

    let mut text = String::from_utf8_lossy(&buf).to_string();
    if start > 0 {
        if let Some(nl) = text.find('\n') {
            text = text[nl + 1..].to_string();
        } else {
            text.insert_str(0, "…(earlier output trimmed)\n");
        }
    }

    Ok(text)
}

pub struct Spec {
    pub session_id: Option<String>,
    pub command: String,
    pub cwd: Option<String>,
    pub profile: sandbox::Profile,
    pub privileged: bool,
    pub permission: String,
    pub label: String,
    pub wake: bool,
    pub timeout_secs: Option<u64>,
}

pub fn spawn<R: tauri::Runtime>(
    app: &AppHandle<R>,
    gw: &Gateway,
    spec: Spec,
) -> Result<Job, String> {
    let id = uuid::Uuid::new_v4().to_string();
    let path = log_path(gw, &id);

    {
        let conn = gw.conn.lock().map_err(|e| e.to_string())?;

        conn.execute(
            "INSERT INTO jobs (id, session_id, label, command, cwd, profile, privilege,
                               state, wake, permission, started_ms, log_path)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'running', ?8, ?9, ?10, ?11)",
            params![
                id,
                spec.session_id,
                spec.label,
                spec.command,
                spec.cwd,
                spec.profile.as_str(),
                if spec.privileged { "admin" } else { "user" },
                spec.wake as i64,
                spec.permission,
                now_ms(),
                path.to_string_lossy().into_owned(),
            ],
        )
        .map_err(|e| e.to_string())?;
    }

    let cancel = Arc::new(Notify::new());

    if let Ok(mut jobs) = gw.jobs.lock() {
        jobs.insert(id.clone(), cancel.clone());
    }

    let app2 = app.clone();
    let gid = id.clone();
    let log = path.clone();

    tauri::async_runtime::spawn(async move {
        supervise(&app2, &gid, spec, &log, cancel).await;
    });

    let conn = gw.conn.lock().map_err(|e| e.to_string())?;
    get(&conn, &id)
}

async fn supervise<R: tauri::Runtime>(
    app: &AppHandle<R>,
    id: &str,
    spec: Spec,
    log: &PathBuf,
    cancel: Arc<Notify>,
) {
    let started = Instant::now();
    let Spec {
        session_id,
        command,
        cwd,
        profile,
        privileged,
        permission,
        label,
        wake,
        timeout_secs,
    } = spec;

    let sink = crate::tools::shell::open_log(log).ok();

    let req = sandbox::Request {
        tool: "terminal",
        command: &command,
        profile,
        cwd: cwd.as_deref(),
        elevated: privileged,
        permission: &permission,
        timeout_secs,
        origin: None,
        background: true,
        log: sink.clone(),
    };

    let gw = app.state::<Gateway>();
    let out = crate::tools::shell::CANCEL
        .scope(cancel, sandbox::run(gw.inner(), req, None))
        .await;

    let (state, exit, note, body) = match out {
        Ok(o) => {
            let s = if o.termination == sandbox::Termination::Cancelled {
                "killed"
            } else if o.succeeded() {
                "done"
            } else {
                "failed"
            };

            let n = if o.termination == sandbox::Termination::TimedOut {
                Some(format!("hit the {}-second deadman", DEADMAN_MAX))
            } else {
                None
            };

            (s, Some(o.exit), n, o.combined())
        }
        Err(err) => {
            if matches!(err, sandbox::plan::SandboxError::Cancelled) {
                ("killed", Some(-2), None, String::new())
            } else {
                let msg = format!("job could not start: {err}");
                ("failed", Some(-1), Some(msg.clone()), msg)
            }
        }
    };

    let ms = started.elapsed().as_millis() as i64;
    let bytes = body.len() as i64;
    let truncated = body.len() > TAIL_MAX;

    if sink.is_none() || std::fs::metadata(log).map(|m| m.len()).unwrap_or(0) == 0 {
        let _ = std::fs::write(log, &body);
    }

    if let Ok(conn) = gw.conn.lock() {
        let _ = set_state(
            &conn,
            id,
            state,
            exit,
            Some(ms),
            bytes,
            truncated,
            note.as_deref(),
        );
    }

    if let Ok(mut jobs) = gw.jobs.lock() {
        jobs.remove(id);
    }

    let sid = match &session_id {
        Some(s) => s.clone(),
        None => return,
    };

    let summary = tail(log, WAKE_TAIL).unwrap_or_default();
    let code = exit.unwrap_or(-1);
    let verdict = match state {
        "done" => "succeeded",
        "killed" => "was stopped before it finished",
        "failed" => "failed to run",
        _ => "did not complete",
    };

    crate::sessions::chat::announce(
        app,
        &gw,
        &sid,
        &format!(
            "<job-done id=\"{id}\" label=\"{}\" state=\"{state}\" exit={code} \
             duration_ms={ms}>\n\
             The background job you started {verdict} (exit {code}).\n\
             Its output is on disk. Read it with job.read before drawing any conclusion \
             about whether the work actually succeeded — the exit code alone rarely says.\n\n\
             {summary}\n</job-done>",
            crate::sessions::chat::attr_escape(&label),
        ),
        wake,
    );

    gw.publish(
        &sid,
        StreamEvent::Notice {
            msg: format!("background job finished: {label} ({state})"),
        },
    );

    let _ = app.emit(
        "job-done",
        JobDone {
            id: id.to_string(),
            label: label.to_string(),
            state: state.to_string(),
            exit,
        },
    );
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct JobDone {
    pub id: String,
    pub label: String,
    pub state: String,
    pub exit: Option<i64>,
}

pub fn kill(gw: &Gateway, id: &str) -> Result<bool, String> {
    let handle = gw.jobs.lock().map_err(|e| e.to_string())?.get(id).cloned();

    match handle {
        Some(n) => {
            n.notify_one();
            Ok(true)
        }
        None => {
            let conn = gw.conn.lock().map_err(|e| e.to_string())?;
            let state: String = conn
                .query_row("SELECT state FROM jobs WHERE id = ?1", params![id], |r| {
                    r.get(0)
                })
                .map_err(|_| format!("no job {id}"))?;

            if state == "running" {
                return Err("that job is running but has no live handle — restart Argus".into());
            }

            Ok(false)
        }
    }
}

pub fn purge(gw: &Gateway, id: &str) -> Result<(), String> {
    let conn = gw.conn.lock().map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM jobs WHERE id = ?1", params![id])
        .map_err(|e| e.to_string())?;

    let _ = std::fs::remove_file(log_path(gw, id));

    Ok(())
}

pub fn cleanup(gw: &Gateway, keep: usize) -> Result<usize, String> {
    let conn = gw.conn.lock().map_err(|e| e.to_string())?;
    let gone: Vec<String> = {
        let mut stmt = conn
            .prepare("SELECT id FROM jobs WHERE state != 'running' ORDER BY started_ms DESC, rowid DESC LIMIT -1 OFFSET ?1")
            .map_err(|e| e.to_string())?;

        let rows = stmt
            .query_map(params![keep as i64], |r| r.get::<_, String>(0))
            .map_err(|e| e.to_string())?;

        rows.flatten().collect()
    };

    for id in &gone {
        let _ = conn.execute("DELETE FROM jobs WHERE id = ?1", params![id]);
        let _ = std::fs::remove_file(log_path(gw, id));
    }

    Ok(gone.len())
}

pub fn duration_since(ms: i64) -> Duration {
    Duration::from_millis((now_ms() - ms).max(0) as u64)
}

#[tauri::command]
pub fn job_list(
    gw: tauri::State<'_, Gateway>,
    session_id: Option<String>,
    limit: Option<i64>,
) -> Result<Vec<Job>, String> {
    let conn = gw.conn.lock().map_err(|e| e.to_string())?;
    list(&conn, session_id.as_deref(), limit.unwrap_or(20))
}

#[tauri::command]
pub fn job_read(
    gw: tauri::State<'_, Gateway>,
    id: String,
    max_chars: Option<i64>,
) -> Result<JobRead, String> {
    let conn = gw.conn.lock().map_err(|e| e.to_string())?;
    let job = get(&conn, &id)?;

    drop(conn);

    let max = max_chars.unwrap_or(TAIL_MAX as i64).clamp(200, 400_000) as usize;

    Ok(JobRead {
        job,
        text: tail(&log_path(gw.inner(), &id), max)?,
    })
}

#[tauri::command]
pub fn job_kill(gw: tauri::State<'_, Gateway>, id: String) -> Result<bool, String> {
    kill(&gw, &id)
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct JobRead {
    pub job: Job,
    pub text: String,
}
