use std::time::Duration;

use rusqlite::{params, Connection};
use tauri::ipc::Channel;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::gateway::router;
use crate::gateway::schema::{ChatReq, StreamEvent, WireMsg};
use crate::gateway::{EventSink, Gateway};
use crate::prompt::{compressor, project};
use crate::tools;

use super::blocks;
use super::guards;
use super::reflect;
use super::schema::NewMsg;
use super::sink;
use super::store;

const DEFAULT_TITLE: &str = "New chat";
pub const MAX_STEPS: usize = 24;
pub const RESULT_CLIP: usize = 4000;
const TERM_TIMEOUT: u64 = 300;

const WATCH_IDLE: std::time::Duration = std::time::Duration::from_millis(150);

/// The turn is already over; this only bounds the relay wait.
const FWD_DRAIN: std::time::Duration = std::time::Duration::from_secs(5);

const WAKE_SLOTS: u32 = 240;
const WAKE_SLOT_MS: u64 = 500;
pub const DENIED_CODE: i64 = -2;
pub const MAX_CLAIM_NUDGES: usize = 2;
pub const MAX_TRUNC_CONTS: usize = 2;

pub const NUDGE: &str = "Your last reply neither ran a tool nor closed the turn. Do not \
resend it: reply now with <final/> on its own last line and nothing else. If you \
meant to act, make the tool call instead and end the reply right after it.";

/// A no-tool answer at least this long reads as an answer, not a status
/// stub: "Working on it." still nudges, a full answer missing only its close
/// marker is accepted instead of sent back and resent whole.
pub const MIN_CLOSE_CHARS: usize = 100;

pub const SUMMARY_DEMAND: &str = "Your last replies kept ending without closing the turn. \
Do not emit any more tool blocks. Reply now with a plain-text summary of what was \
actually accomplished in this turn and what is still left to do, then close it with \
<final/> on its own last line.";

pub const TRUNC_CONT: &str = "Your previous reply was cut off at the model's output limit. \
Continue with the next step now. If a tool-result already arrived for an action, \
that work is done — do not repeat it. Only if you were in the middle of an action \
block that has no matching tool-result yet, re-emit that whole block from its start.";

pub const EMPTY_CONT: &str = "Your last reply was empty. Continue with the task now; to act, \
end your reply with an action block.";

pub const HARD_STEPS: usize = 6;

pub const SKILL_NUDGE: &str =
    "That took real work. If the task succeeded and any part of it is reusable, \
save it as a skill now: first skill.search for overlap, then skill.create with a kebab-case name, \
a one-line description, and a body of When to use, Steps, and Pitfalls sections. \
If nothing here is worth reusing, say so in one line and finish.";

pub fn auto_model(conn: &Connection) -> Result<String, String> {
    let models = crate::gateway::store::list_chat_models(conn)?;

    models
        .first()
        .map(|m| m.model_id.clone())
        .ok_or("auto mode: no enabled models".into())
}

pub fn approval_id() -> String {
    format!("ap{}", uuid::Uuid::new_v4().as_simple())
}

/// Post a message without running a turn, then tell the open chat to re-read.
/// This is how a card lands mid-answer.
pub fn post(gw: &Gateway, session_id: &str, role: &str, body: &str) {
    if let Ok(conn) = gw.conn.lock() {
        let _ = store::add_msg(
            &conn,
            &NewMsg {
                session_id: session_id.into(),
                role: role.into(),
                content: body.into(),
                model_id: None,
                provider_id: None,
                tok_in: None,
                tok_out: None,
                tool_calls: None,
                tool_call_id: None,
                attachments: None,
            },
        );
    }

    gw.publish(session_id, StreamEvent::Refresh);
}

pub fn announce<R: tauri::Runtime>(
    app: &AppHandle<R>,
    gw: &Gateway,
    session_id: &str,
    body: &str,
    wake: bool,
) {
    if !wake {
        if let Ok(conn) = gw.conn.lock() {
            let _ = store::add_msg(
                &conn,
                &NewMsg {
                    session_id: session_id.into(),
                    role: "system".into(),
                    content: body.into(),
                    model_id: None,
                    provider_id: None,
                    tok_in: None,
                    tok_out: None,
                    tool_calls: None,
                    tool_call_id: None,
                    attachments: None,
                },
            );
        }

        return;
    }

    let app2 = app.clone();
    let sid = session_id.to_string();
    let body2 = body.to_string();

    tauri::async_runtime::spawn(async move {
        for _ in 0..WAKE_SLOTS {
            let gw = app2.state::<Gateway>();

            if gw.claim_turn(&sid) {
                let sink = sink::BusSink {
                    gw: gw.inner(),
                    session_id: sid.clone(),
                };

                // Without this the wake refuses every step needing approval:
                // it thinks it is alone.
                gw.go_live(&sid);

                let _ = crate::tools::shell::CANCEL
                    .scope(
                        std::sync::Arc::new(tokio::sync::Notify::new()),
                        crate::tools::notepad::SESSION_ID.scope(
                            Some(sid.clone()),
                            send(gw.inner(), &app2, &sid, &body2, None, &sink, "system"),
                        ),
                    )
                    .await;

                gw.go_quiet(&sid);
                gw.release_turn(&sid);
                return;
            }

            tokio::time::sleep(std::time::Duration::from_millis(WAKE_SLOT_MS)).await;
        }

        let gw = app2.state::<Gateway>();
        let conn = gw.conn.lock();

        if let Ok(conn) = conn {
            let _ = store::add_msg(
                &conn,
                &NewMsg {
                    session_id: sid.clone(),
                    role: "system".into(),
                    content: body2.clone(),
                    model_id: None,
                    provider_id: None,
                    tok_in: None,
                    tok_out: None,
                    tool_calls: None,
                    tool_call_id: None,
                    attachments: None,
                },
            );
        }
    });
}

pub async fn ask_approval(
    gw: &Gateway,
    sink: &dyn sink::ChatSink,
    id: &str,
    idx: u32,
    cmd: &str,
) -> crate::gateway::ApprovalReply {
    let (tx, rx) = tokio::sync::oneshot::channel::<crate::gateway::ApprovalReply>();

    if let Ok(mut map) = gw.approvals.lock() {
        map.insert(id.into(), tx);
    }

    sink.emit(StreamEvent::Approval {
        id: id.into(),
        idx,
        command: cmd.into(),
    });

    let reply = match tokio::time::timeout(Duration::from_secs(TERM_TIMEOUT), rx).await {
        Ok(Ok(r)) => r,
        _ => crate::gateway::ApprovalReply {
            allow: false,
            args: None,
        },
    };

    if let Ok(mut map) = gw.approvals.lock() {
        map.remove(id);
    }

    reply
}

pub async fn run_skill_reflection<R: tauri::Runtime>(app: &AppHandle<R>, session_id: &str) {
    let gw = app.state::<Gateway>();

    let req: ChatReq = {
        let Ok(conn) = gw.conn.lock() else { return };
        let Ok(mut p) = project(&conn, session_id, &gw.library_dir) else {
            return;
        };

        if p.model_id.is_none() {
            match auto_model(&conn) {
                Ok(m) => p.model_id = Some(m),
                Err(_) => return,
            }
        }

        let mut r = p.chat_req();
        blocks::attach_shots(&mut r.msgs);
        r.msgs.push(WireMsg {
            role: "user".into(),
            content: SKILL_NUDGE.into(),
            ..Default::default()
        });
        r
    };

    let quiet = EventSink::null();

    let Ok(stats) = router::stream_run(&gw, req, &quiet).await else {
        return;
    };

    let base_text = blocks::sanitize_tags(&tools::normalize_actions(&stats.text));
    let execs = tools::build_executions(&base_text, &stats.tool_calls, 0);

    for e in execs {
        if e.tool != "skill.create" {
            continue;
        }

        let _ = tools::recover::exec_with_recovery(tools::ExecIn {
            app,
            gw: &gw,
            name: &e.tool,
            args_json: &e.args,
            permission: "never",
            web: false,
            approved: true,
            on_term: None,
        })
        .await;
    }
}

pub async fn send<R: tauri::Runtime>(
    gw: &Gateway,
    app: &AppHandle<R>,
    session_id: &str,
    content: &str,
    attachments: Option<&str>,
    sink: &dyn sink::ChatSink,
    role: &str,
) -> Result<(), String> {
    let out = turn(gw, app, session_id, content, attachments, sink, role).await;

    // The forwarder only releases the session lock on TurnEnd, so this cannot
    // sit behind a `?`.
    if let Err(err) = &out {
        let kind = crate::gateway::router::fail_kind(err);
        // A failed turn persists nothing, so the reload after TurnEnd shows no
        // bubble. Every provider error belongs in the transcript instead.
        if let Ok(conn) = gw.conn.lock() {
            let body = err.replace('&', "&amp;").replace('<', "&lt;");
            let content = format!(
                "<error severity=\"high\" kind=\"{kind}\">{}</error>",
                body.trim()
            );

            if let Ok(row) = store::add_msg(
                &conn,
                &NewMsg {
                    session_id: session_id.into(),
                    role: "assistant".into(),
                    content,
                    model_id: None,
                    provider_id: None,
                    tok_in: None,
                    tok_out: None,
                    tool_calls: None,
                    tool_call_id: None,
                    attachments: None,
                },
            ) {
                let _ = store::mark_final(&conn, &row.id);
            }
        }

        sink.emit(StreamEvent::Err {
            msg: err.clone(),
            kind: kind.into(),
        });
    }

    sink.emit(StreamEvent::TurnEnd {
        session_id: session_id.into(),
    });

    out
}

async fn turn<R: tauri::Runtime>(
    gw: &Gateway,
    app: &AppHandle<R>,
    session_id: &str,
    content: &str,
    attachments: Option<&str>,
    sink: &dyn sink::ChatSink,
    role: &str,
) -> Result<(), String> {
    {
        let conn = gw.conn.lock().map_err(|err| err.to_string())?;
        let dupe = store::has_unreplied_duplicate(&conn, session_id, content).unwrap_or(false);
        if !dupe {
            store::add_msg(
                &conn,
                &NewMsg {
                    session_id: session_id.into(),
                    role: role.into(),
                    content: content.into(),
                    model_id: None,
                    provider_id: None,
                    tok_in: None,
                    tok_out: None,
                    tool_calls: None,
                    tool_call_id: None,
                    attachments: attachments.map(str::to_string),
                },
            )?;
        }
        // The previous turn's resume stays in place; the first `project()` below
        // is its only reader, and it is overwritten on every unfinished stop.
    }

    let model_chan = sink.event_sink().unwrap_or_else(EventSink::null);

    let (perm, web) = {
        let conn = gw.conn.lock().map_err(|err| err.to_string())?;
        conn.query_row(
            "SELECT permission, web_search FROM sessions WHERE id = ?1",
            params![session_id],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? != 0)),
        )
        .unwrap_or_else(|_| ("ask".into(), false))
    };

    let allow_hosts: Vec<String> = {
        let conn = gw.conn.lock().map_err(|err| err.to_string())?;
        crate::gateway::store::kv_get(&conn, crate::tools::sandbox::KV_ALLOW_HOSTS)
            .and_then(|v| serde_json::from_str(&v).ok())
            .unwrap_or_default()
    };
    let turn_budget = {
        let conn = gw.conn.lock().map_err(|err| err.to_string())?;
        // Sized off the model's own window, never off `sessions.ctx_tokens`:
        // that column holds the previous turn's summed spend, so basing the
        // budget on it collapsed every turn after the first to the floor and
        // cut long runs off after two or three steps.
        let model_id: Option<String> = conn
            .query_row(
                "SELECT model_id FROM sessions WHERE id = ?1",
                params![session_id],
                |r| r.get(0),
            )
            .unwrap_or(None);
        let window = crate::prompt::config::context_window(&conn, model_id.as_deref());
        guards::turn_budget(window)
    };

    let mut turn = super::turn::Turn::new();
    turn.run(
        gw,
        app,
        session_id,
        sink,
        &model_chan,
        &perm,
        web,
        &allow_hosts,
        turn_budget,
    )
    .await?;

    let finished = turn.finished;
    let tok_in_sum = turn.tok_in_sum;
    let recent = turn.recent;

    {
        // `recent` is dead after the turn; move the names out, no clone.
        let tools_seen: Vec<String> = recent.into_iter().map(|(t, _)| t).collect();
        let mut ep =
            crate::memory::session_memory::session_memory_from_turn(content, &tools_seen, finished);
        ep.session_id = session_id.into();
        if let Ok(conn) = gw.conn.lock() {
            let _ = crate::memory::session_memory::save_session_memory(&conn, &ep);
        }
    }

    {
        let conn = gw.conn.lock().map_err(|err| err.to_string())?;
        store::touch_session(&conn, session_id, tok_in_sum)?;
    }

    let needs = {
        let conn = gw.conn.lock().map_err(|err| err.to_string())?;
        compressor::check(&conn, session_id, &gw.library_dir)
            .map(|s| s.needs_compact)
            .unwrap_or(false)
    };

    if needs {
        let _ = compressor::compact(gw, session_id).await;
    }

    let untitled = {
        let conn = gw.conn.lock().map_err(|err| err.to_string())?;
        conn.query_row(
            "SELECT title FROM sessions WHERE id = ?1",
            params![session_id],
            |r| r.get::<_, String>(0),
        )
        .map(|t| t == DEFAULT_TITLE)
        .unwrap_or(false)
    };

    if untitled {
        let temp = reflect::clean_title(content).unwrap_or_else(|| DEFAULT_TITLE.into());

        {
            let conn = gw.conn.lock().map_err(|err| err.to_string())?;
            conn.execute(
                "UPDATE sessions SET title = ?2 WHERE id = ?1",
                params![session_id, temp],
            )
            .map_err(|err| err.to_string())?;
        }

        let app = app.clone();
        let sid = session_id.to_string();
        let user_text = content.to_string();

        tauri::async_runtime::spawn(async move {
            let gw = app.state::<Gateway>();
            let _ = reflect::generate_title(gw.inner(), &sid, &user_text).await;
            let _ = app.emit("sessions-changed", ());
        });
    }

    let _ = app.emit(
        "argus://session-activity",
        serde_json::json!({"session_id": session_id, "kind": "turn-done"}),
    );

    Ok(())
}

#[tauri::command]
pub async fn sess_chat_stream(
    gw: State<'_, Gateway>,
    app: AppHandle,
    session_id: String,
    content: String,
    attachments: Option<String>,
    on_event: Channel<StreamEvent>,
) -> Result<(), String> {
    let notify = std::sync::Arc::new(tokio::sync::Notify::new());

    if !gw.claim_turn(&session_id) {
        return Err("this session is already answering — wait for it to finish".into());
    }

    if let Ok(mut tasks) = gw.tasks.lock() {
        tasks.insert(session_id.clone(), notify.clone());
    }

    gw.go_live(&session_id);

    let mut rx = gw.subscribe(&session_id);
    let fwd = tauri::async_runtime::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(StreamEvent::TurnEnd { .. }) => {
                    let _ = on_event.send(StreamEvent::TurnEnd {
                        session_id: String::new(),
                    });
                    break;
                }
                Ok(ev) => {
                    if on_event.send(ev).is_err() {
                        break;
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    });

    let sink = sink::BusSink {
        gw: gw.inner(),
        session_id: session_id.clone(),
    };

    let out = tokio::select! {
        _ = notify.notified() => Err("stopped".into()),
        out = crate::tools::shell::CANCEL.scope(notify.clone(), crate::tools::notepad::SESSION_ID.scope(Some(session_id.clone()), send(&gw, &app, &session_id, &content, attachments.as_deref(), &sink, "user"))) => out,
    };

    // A bus busier than BUS_CAP can lag the last event away, so drain it.
    let _ = tokio::time::timeout(FWD_DRAIN, fwd).await;

    if let Ok(mut tasks) = gw.tasks.lock() {
        tasks.remove(&session_id);
    }

    gw.go_quiet(&session_id);
    gw.drop_bus(&session_id);
    gw.release_turn(&session_id);

    out
}

#[tauri::command]
pub async fn sess_watch_events(
    gw: State<'_, Gateway>,
    session_id: String,
    on_event: Channel<StreamEvent>,
) -> Result<(), String> {
    let mut rx = gw.subscribe(&session_id);
    gw.start_watching(&session_id);

    while gw.watching(&session_id) {
        let ev = match tokio::time::timeout(WATCH_IDLE, rx.recv()).await {
            Ok(Ok(ev)) => ev,
            Ok(Err(tokio::sync::broadcast::error::RecvError::Lagged(_))) => continue,
            Ok(Err(tokio::sync::broadcast::error::RecvError::Closed)) => break,
            Err(_) => continue,
        };

        if on_event.send(ev).is_err() {
            break;
        }
    }

    gw.stop_watching(&session_id);
    gw.drop_bus(&session_id);

    Ok(())
}

#[tauri::command]
pub fn sess_unwatch(gw: State<'_, Gateway>, session_id: Option<String>) {
    match session_id {
        Some(id) => gw.stop_watching(&id),
        None => gw.stop_watching_all(),
    }
}

#[tauri::command]
pub fn sess_cancel_chat(gw: State<'_, Gateway>, session_id: String) -> Result<bool, String> {
    let took = gw
        .tasks
        .lock()
        .map_err(|err| err.to_string())?
        .remove(&session_id);

    match took {
        Some(n) => {
            n.notify_one();
            Ok(true)
        }
        None => Ok(false),
    }
}

#[tauri::command]
pub fn sess_resolve_approval(
    gw: State<'_, Gateway>,
    approval_id: String,
    allow: bool,
    args: Option<String>,
) -> Result<(), String> {
    let reply = crate::gateway::approval_reply(allow, args)?;
    let tx = gw
        .approvals
        .lock()
        .map_err(|err| err.to_string())?
        .remove(&approval_id);

    match tx {
        Some(tx) => {
            let _ = tx.send(reply);
            Ok(())
        }
        None => Err("unknown approval".into()),
    }
}
