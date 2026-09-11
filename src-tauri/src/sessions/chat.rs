use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use rusqlite::{params, Connection};
use tauri::ipc::Channel;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::gateway::router;
use crate::gateway::schema::{ChatReq, StreamEvent, WireMsg};
use crate::gateway::Gateway;
use crate::prompt::config::truncate_chars;
use crate::prompt::{compressor, project};
use crate::tools;

use super::schema::NewMsg;
use super::store;

const DEFAULT_TITLE: &str = "New chat";
const MAX_STEPS: usize = 12;
const RESULT_CLIP: usize = 4000;
const TERM_TIMEOUT: u64 = 300;
const DENIED_CODE: i64 = -2;

static APPROVAL_SEQ: AtomicU64 = AtomicU64::new(0);

const TITLE_SYS: &str =
    "You are the title generator for Argus, a personal AI agent the user chats with. \
Write a short session title for the user's message. Reply with only the title: \
3 to 6 words, no quotes, no trailing punctuation.";

const NUDGE: &str = "Continue: your last reply said you were acting, but it contained no \
<action> block, so nothing actually ran. Emit the correct action block now. If you \
cannot act, say so plainly — never describe an action without running it.";

/// Catches the "I'm opening X…" failure mode: intent phrasing with no action
/// block behind it, so the turn would end with nothing done.
pub fn claims_action(text: &str) -> bool {
    let re = regex::Regex::new(
        r"(?i)\b(i'?m|i am|i'?ll|i will|let me|going to)\s+(open|click|type|run|search|navigat|check|launch|browse)\w*",
    );

    re.map(|r| r.is_match(text)).unwrap_or(false)
}

async fn generate_title(gw: &Gateway, session_id: &str, content: &str) -> Result<(), String> {
    let (util, selected) = {
        let conn = gw.conn.lock().map_err(|e| e.to_string())?;
        let selected: Option<String> = conn
            .query_row(
                "SELECT model_id FROM sessions WHERE id = ?1",
                params![session_id],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        (compressor::utility_model(&conn), selected)
    };

    let msgs = vec![
        WireMsg {
            role: "system".into(),
            content: TITLE_SYS.into(),
        },
        WireMsg {
            role: "user".into(),
            content: truncate_chars(content, 500),
        },
    ];

    let raw = match util {
        Ok(u) => {
            let req = ChatReq {
                model: u,
                msgs: msgs.clone(),
                prefix_hash: None,
            };
            match router::run_opts(gw, &req, 2).await {
                Ok(resp) => resp.content,
                Err(_) => fallback_title(gw, selected, &msgs).await?,
            }
        }
        Err(_) => fallback_title(gw, selected, &msgs).await?,
    };

    let title = clean_title(&raw).ok_or("title model returned nothing usable")?;

    let conn = gw.conn.lock().map_err(|e| e.to_string())?;
    conn.execute(
        "UPDATE sessions SET title = ?2 WHERE id = ?1",
        params![session_id, title],
    )
    .map_err(|e| e.to_string())?;

    Ok(())
}

async fn fallback_title(
    gw: &Gateway,
    selected: Option<String>,
    msgs: &[WireMsg],
) -> Result<String, String> {
    let model = selected
        .filter(|m| !m.is_empty())
        .ok_or("no fallback model for title")?;

    let req = ChatReq {
        model,
        msgs: msgs.to_vec(),
        prefix_hash: None,
    };

    Ok(router::run_opts(gw, &req, 2).await?.content)
}

pub fn clean_title(raw: &str) -> Option<String> {
    let line = raw.lines().next().unwrap_or("").trim();
    let t = line.trim_matches('"').trim_matches('\'').trim();

    if t.is_empty() || t.contains('<') || t.contains('>') {
        return None;
    }

    Some(truncate_chars(t, 60))
}

fn auto_model(conn: &Connection) -> Result<String, String> {
    let models = crate::gateway::store::list_chat_models(conn)?;

    models
        .first()
        .map(|m| m.model_id.clone())
        .ok_or("auto mode: no enabled models".into())
}

fn approval_id() -> String {
    let n = APPROVAL_SEQ.fetch_add(1, Ordering::Relaxed);
    let t = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);

    format!("ap{t}-{n}")
}

async fn ask_approval(
    gw: &Gateway,
    chan: &Channel<StreamEvent>,
    id: &str,
    idx: u32,
    cmd: &str,
) -> bool {
    let (tx, rx) = tokio::sync::oneshot::channel::<bool>();

    if let Ok(mut map) = gw.approvals.lock() {
        map.insert(id.into(), tx);
    }

    let _ = chan.send(StreamEvent::Approval {
        id: id.into(),
        idx,
        command: cmd.into(),
    });

    let allow = matches!(
        tokio::time::timeout(Duration::from_secs(TERM_TIMEOUT), rx).await,
        Ok(Ok(true))
    );

    if let Ok(mut map) = gw.approvals.lock() {
        map.remove(id);
    }

    allow
}

fn esc_attr(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
}

fn terminal_block(idx: usize, cmd: &str, code: i64, out: &str) -> String {
    let status = if code == 0 { "ok" } else { "error" };
    let body = out.replace('&', "&amp;").replace('<', "&lt;");

    format!(
        "<terminal id=\"a{idx}\" command=\"{}\" status=\"{status}\">{}</terminal>",
        esc_attr(cmd),
        body.trim()
    )
}

fn exit_of(body: &str) -> i64 {
    body.strip_prefix("exit ")
        .and_then(|r| r.split_once('\n'))
        .and_then(|(c, _)| c.parse::<i64>().ok())
        .unwrap_or(-1)
}

fn browser_what(tool: &str, v: &serde_json::Value, masked: bool) -> String {
    let text = v.get("text").and_then(|t| t.as_str()).map(|s| {
        if masked {
            "····".into()
        } else {
            s.to_string()
        }
    });
    let what = v
        .get("url")
        .and_then(|u| u.as_str())
        .map(Into::into)
        .or(text)
        .unwrap_or_default();

    match v.get("ref").and_then(|r| r.as_u64()) {
        Some(r) => format!("{tool} ref {r} {what}"),
        None => format!("{tool} {what}"),
    }
}

fn browser_block(idx: usize, tool: &str, url: &str, what: &str) -> String {
    format!(
        "<browser-action id=\"a{idx}\" url=\"{}\" action=\"{}\">{}</browser-action>",
        esc_attr(url),
        esc_attr(tool),
        esc_attr(what)
    )
}

fn body_url(body: &str) -> String {
    body.lines()
        .find(|l| l.starts_with("url "))
        .map(|l| l[4..].trim().to_string())
        .unwrap_or_default()
}

fn sanitize_tags(s: &str) -> String {
    let mut t = s.to_string();
    t = t
        .replace("<strong>", "<bold>")
        .replace("</strong>", "</bold>");
    t = t.replace("<b>", "<bold>").replace("</b>", "</bold>");
    t = t.replace("<em>", "<italic>").replace("</em>", "</italic>");
    t = t.replace("<i>", "<italic>").replace("</i>", "</italic>");
    t = t
        .replace("<u>", "<underline>")
        .replace("</u>", "</underline>");
    t = t
        .replace("<a ", "<link ")
        .replace("<a>", "<link>")
        .replace("</a>", "</link>");

    if let Ok(re) = regex::Regex::new(r"(?i)</?(p|div|span|command|output)[^>]*>") {
        t = re.replace_all(&t, "").into_owned();
    }

    if let Ok(re) = regex::Regex::new(r"(?i)<br\s*/?>") {
        t = re.replace_all(&t, "\n").into_owned();
    }

    t
}

pub async fn send(
    gw: &Gateway,
    app: &AppHandle,
    session_id: &str,
    content: &str,
    chan: &Channel<StreamEvent>,
) -> Result<(), String> {
    {
        let conn = gw.conn.lock().map_err(|e| e.to_string())?;
        store::add_msg(
            &conn,
            &NewMsg {
                session_id: session_id.into(),
                role: "user".into(),
                content: content.into(),
                model_id: None,
                provider_id: None,
                tok_in: None,
                tok_out: None,
            },
        )?;
    }

    let (perm, web) = {
        let conn = gw.conn.lock().map_err(|e| e.to_string())?;
        conn.query_row(
            "SELECT permission, web_search FROM sessions WHERE id = ?1",
            params![session_id],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? != 0)),
        )
        .unwrap_or_else(|_| ("ask".into(), false))
    };

    let mut tok_in_sum = 0i64;
    let mut act_base = 0usize;
    let mut nudge: Option<String> = None;
    let mut nudged = false;

    for _step in 0..MAX_STEPS {
        let mut req = {
            let conn = gw.conn.lock().map_err(|e| e.to_string())?;
            let mut p = project(&conn, session_id)?;
            if p.model_id.is_none() {
                p.model_id = Some(auto_model(&conn)?);
            }

            let mut r = p.chat_req();

            if let Some(n) = &nudge {
                r.msgs.push(WireMsg {
                    role: "user".into(),
                    content: n.clone(),
                });
            }

            r.prefix_hash = Some(p.prefix_hash);
            r
        };

        let stats = router::stream_run(gw, &req, chan).await?;
        req.msgs.clear();
        tok_in_sum += stats.tok_in;

        if stats.text.trim().is_empty() {
            return Err("model returned an empty reply — try again".into());
        }

        let text = sanitize_tags(&tools::normalize_actions(&stats.text));
        let actions = tools::parse_actions(&text);
        let done = actions.is_empty();

        let asst = {
            let conn = gw.conn.lock().map_err(|e| e.to_string())?;
            store::add_msg(
                &conn,
                &NewMsg {
                    session_id: session_id.into(),
                    role: "assistant".into(),
                    content: text.clone(),
                    model_id: Some(stats.model_id),
                    provider_id: Some(stats.provider_id),
                    tok_in: Some(stats.tok_in),
                    tok_out: Some(stats.tok_out),
                },
            )?
        };

        if done {
            if !nudged && claims_action(&text) {
                nudged = true;
                nudge = Some(NUDGE.into());
                continue;
            }

            break;
        }

        let _ = chan.send(StreamEvent::Step);

        let mut edits: Vec<(usize, usize, String)> = vec![];

        for (i, a) in actions.iter().enumerate() {
            let idx = act_base + i;
            let is_term = a.tool == "terminal" || a.tool == "bash.run";
            let is_browser = a.tool.starts_with("browser.");

            let args_v: serde_json::Value =
                serde_json::from_str(&a.args).unwrap_or(serde_json::Value::Null);

            let cmd = if is_term {
                args_v["command"].as_str().unwrap_or_default().to_string()
            } else {
                String::new()
            };

            let sensitive = is_browser && tools::browser::sensitive(&a.tool, &args_v).await;
            let needs_ask = (perm == "ask" && tools::is_mutating(&a.tool)) || sensitive;

            let mut allow = !needs_ask;
            let mut denied = false;

            if !allow {
                let what = if is_browser {
                    browser_what(&a.tool, &args_v, false)
                } else {
                    cmd.clone()
                };

                allow = ask_approval(gw, &chan, &approval_id(), idx as u32, &what).await;
                denied = !allow;

                if denied && is_term {
                    let _ = chan.send(StreamEvent::TermEnd {
                        idx: idx as u32,
                        code: DENIED_CODE,
                    });
                }
            }

            let (status, body, code) = if denied {
                ("err", "action denied by user".to_string(), DENIED_CODE)
            } else {
                match tools::exec(
                    &a.tool,
                    &a.args,
                    &perm,
                    web,
                    allow,
                    Some((&chan, idx as u32)),
                )
                .await
                {
                    Ok(t) => {
                        let code = exit_of(&t);
                        ("ok", t, code)
                    }
                    Err(e) => ("err", e, -1),
                }
            };

            let msg = format!(
                "<tool-result tool=\"{}\" status=\"{}\">{}</tool-result>",
                a.tool,
                status,
                truncate_chars(&body, RESULT_CLIP)
            );

            {
                let conn = gw.conn.lock().map_err(|e| e.to_string())?;
                store::add_msg(
                    &conn,
                    &NewMsg {
                        session_id: session_id.into(),
                        role: "user".into(),
                        content: msg,
                        model_id: None,
                        provider_id: None,
                        tok_in: None,
                        tok_out: None,
                    },
                )?;
            }

            if is_term {
                if status == "ok" {
                    let _ = chan.send(StreamEvent::TermEnd {
                        idx: idx as u32,
                        code,
                    });
                }

                let out = if denied {
                    "command denied by user".to_string()
                } else {
                    body.strip_prefix("exit ")
                        .and_then(|r| r.split_once('\n'))
                        .map(|(_, o)| o.to_string())
                        .unwrap_or_else(|| body.clone())
                };

                edits.push((a.start, a.end, terminal_block(idx, &cmd, code, &out)));
            }

            if is_browser {
                let url = args_v["url"]
                    .as_str()
                    .map(Into::into)
                    .unwrap_or_else(|| body_url(&body));

                edits.push((
                    a.start,
                    a.end,
                    browser_block(idx, &a.tool, &url, &browser_what(&a.tool, &args_v, true)),
                ));
            }
        }

        if !edits.is_empty() {
            let mut updated = text;

            for (s, e, blk) in edits.into_iter().rev() {
                updated.replace_range(s..e, &blk);
            }

            let conn = gw.conn.lock().map_err(|e| e.to_string())?;
            conn.execute(
                "UPDATE messages SET content = ?2 WHERE id = ?1",
                params![&asst.id, &updated],
            )
            .map_err(|e| e.to_string())?;
        }

        act_base += actions.len();
    }

    {
        let conn = gw.conn.lock().map_err(|e| e.to_string())?;
        store::touch_session(&conn, session_id, tok_in_sum)?;
    }

    let needs = {
        let conn = gw.conn.lock().map_err(|e| e.to_string())?;
        compressor::check(&conn, session_id)
            .map(|s| s.needs_compact)
            .unwrap_or(false)
    };

    if needs {
        if let Err(e) = compressor::compact(gw, session_id).await {
            eprintln!("compaction skipped: {e}");
        }
    }

    let untitled = {
        let conn = gw.conn.lock().map_err(|e| e.to_string())?;
        conn.query_row(
            "SELECT title FROM sessions WHERE id = ?1",
            params![session_id],
            |r| r.get::<_, String>(0),
        )
        .map(|t| t == DEFAULT_TITLE)
        .unwrap_or(false)
    };

    if untitled {
        let temp = clean_title(content).unwrap_or_else(|| DEFAULT_TITLE.into());

        {
            let conn = gw.conn.lock().map_err(|e| e.to_string())?;
            conn.execute(
                "UPDATE sessions SET title = ?2 WHERE id = ?1",
                params![session_id, temp],
            )
            .map_err(|e| e.to_string())?;
        }

        let app = app.clone();
        let sid = session_id.to_string();
        let user_text = content.to_string();

        tauri::async_runtime::spawn(async move {
            let gw = app.state::<Gateway>();
            if let Err(e) = generate_title(gw.inner(), &sid, &user_text).await {
                eprintln!("title skipped: {e}");
            }
            let _ = app.emit("sessions-changed", ());
        });
    }

    Ok(())
}

#[tauri::command]
pub async fn sess_chat_stream(
    gw: State<'_, Gateway>,
    app: AppHandle,
    session_id: String,
    content: String,
    on_event: Channel<StreamEvent>,
) -> Result<(), String> {
    send(&gw, &app, &session_id, &content, &on_event).await
}

#[tauri::command]
pub fn sess_resolve_approval(
    gw: State<'_, Gateway>,
    approval_id: String,
    allow: bool,
) -> Result<(), String> {
    let tx = gw
        .approvals
        .lock()
        .map_err(|e| e.to_string())?
        .remove(&approval_id);

    match tx {
        Some(tx) => {
            let _ = tx.send(allow);
            Ok(())
        }
        None => Err("unknown approval".into()),
    }
}
