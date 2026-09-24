use std::time::Duration;

use rusqlite::{params, Connection, OptionalExtension};
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
const MAX_STEPS: usize = 24;
const RESULT_CLIP: usize = 4000;
const TERM_TIMEOUT: u64 = 300;
const DENIED_CODE: i64 = -2;
const MAX_CLAIM_NUDGES: usize = 2;
const MAX_TRUNC_CONTS: usize = 2;
const MAX_REFLECT_NUDGES: usize = 1;

const TITLE_SYS: &str =
    "You are the title generator for Argus, a personal AI agent the user chats with. \
Write a short session title for the user's message. Reply with only the title: \
3 to 6 words, no quotes, no trailing punctuation.";

const NUDGE: &str = "Your last reply neither ran a tool nor closed the turn. A reply ends \
one of exactly two ways: with one or more <action> blocks, or with the final answer \
followed by <final/> on its own last line. If you meant to act, emit the block now and \
end the reply right after it: <action tool=\"...\">{\"arg\":\"...\"}</action>. If you \
cannot act, say so plainly and end with <final/> — never describe an action without \
running it.";

const SUMMARY_DEMAND: &str = "Your last replies kept ending without closing the turn. \
Do not emit any more tool blocks. Reply now with a plain-text summary of what was \
actually accomplished in this turn and what is still left to do, then close it with \
<final/> on its own last line.";

const TRUNC_CONT: &str = "Your previous reply was cut off at the model's output limit. \
Continue with the next step now. If a tool-result already arrived for an action, \
that work is done — do not repeat it. Only if you were in the middle of an action \
block that has no matching tool-result yet, re-emit that whole block from its start.";

const EMPTY_CONT: &str = "Your last reply was empty. Continue with the task now; to act, \
end your reply with an action block.";

const HARD_STEPS: usize = 6;

pub const SKILL_NUDGE: &str =
    "That took real work. If the task succeeded and any part of it is reusable, \
save it as a skill now: first skill.search for overlap, then skill.create with a kebab-case name, \
a one-line description, and a body of When to use, Steps, and Pitfalls sections. \
If nothing here is worth reusing, say so in one line and finish.";

pub fn fakes_output(text: &str) -> bool {
    text.contains("<browser-action") || text.contains("<terminal")
}

pub fn has_faux_sandbox(text: &str) -> bool {
    text.contains("<sandbox")
}

pub fn should_reflect(reflect_on: bool, acts_run: usize, reflect_nudges: usize) -> bool {
    reflect_on && acts_run > 0 && reflect_nudges < MAX_REFLECT_NUDGES
}

pub fn parse_reflection_verdict(text: &str) -> Option<String> {
    let t = text.trim();
    let upper = t.to_ascii_uppercase();

    if upper == "PASS"
        || upper.starts_with("PASS ")
        || upper.starts_with("PASS\n")
        || upper.starts_with("PASS.")
        || upper.starts_with("PASS:")
    {
        return None;
    }

    Some(t.chars().take(2000).collect())
}

pub fn check_block(pass: bool) -> String {
    if pass {
        "<check status=\"pass\"/>".into()
    } else {
        "<check status=\"retry\"/>".into()
    }
}

async fn run_reflection_check(
    gw: &Gateway,
    session_id: &str,
    answer: &str,
) -> Result<Option<String>, String> {
    let (goal, model) = {
        let conn = gw.conn.lock().map_err(|err| err.to_string())?;
        let goal: Option<String> = conn
            .query_row(
                "SELECT content FROM messages WHERE session_id = ?1 AND role = 'user' \
                 AND active = 1 AND content NOT LIKE '<tool-result%' \
                 ORDER BY seq LIMIT 1",
                params![session_id],
                |r| r.get(0),
            )
            .optional()
            .map_err(|err| err.to_string())?;
        let Some(goal) = goal else {
            return Ok(None);
        };
        let model = crate::prompt::compressor::utility_model(&conn)?;
        (goal, model)
    };

    let instruction = format!(
        "You are Argus's answer checker. Does the reply below actually satisfy the goal \
         stated in the first message? Did any tool call actually fail without the reply \
         acknowledging it? Reply with exactly PASS if yes to the first and no surprises, \
         otherwise reply with one short corrective instruction.\n\
         \n\
         GOAL:\n\
         {goal}\n\
         \n\
         REPLY:\n\
         {answer}"
    );

    let request = ChatReq {
        model,
        msgs: vec![WireMsg {
            role: "user".into(),
            content: instruction,
            ..Default::default()
        }],
        prefix_hash: None,
        tools: vec![],
    };

    let response = crate::gateway::router::run_opts(gw, &request, 1).await?;

    Ok(parse_reflection_verdict(&response.content))
}

async fn generate_title(gw: &Gateway, session_id: &str, content: &str) -> Result<(), String> {
    let (util, selected) = {
        let conn = gw.conn.lock().map_err(|err| err.to_string())?;
        let selected: Option<String> = conn
            .query_row(
                "SELECT model_id FROM sessions WHERE id = ?1",
                params![session_id],
                |r| r.get(0),
            )
            .map_err(|err| err.to_string())?;
        (compressor::utility_model(&conn), selected)
    };

    let msgs = vec![
        WireMsg {
            role: "system".into(),
            content: TITLE_SYS.into(),
            ..Default::default()
        },
        WireMsg {
            role: "user".into(),
            content: truncate_chars(content, 500),
            ..Default::default()
        },
    ];

    let raw = match util {
        Ok(u) => {
            let req = ChatReq {
                model: u,
                msgs: msgs.clone(),
                prefix_hash: None,
                tools: vec![],
            };
            match router::run_opts(gw, &req, 2).await {
                Ok(resp) => resp.content,
                Err(_) => fallback_title(gw, selected, &msgs).await?,
            }
        }
        Err(_) => fallback_title(gw, selected, &msgs).await?,
    };

    let title = clean_title(&raw).ok_or("title model returned nothing usable")?;

    let conn = gw.conn.lock().map_err(|err| err.to_string())?;
    conn.execute(
        "UPDATE sessions SET title = ?2 WHERE id = ?1",
        params![session_id, title],
    )
    .map_err(|err| err.to_string())?;

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
        tools: vec![],
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
    format!("ap{}", uuid::Uuid::new_v4().as_simple())
}

pub fn repeated(recent: &[(String, String)], key: &(String, String)) -> bool {
    if recent.iter().rev().take_while(|p| *p == key).count() >= 2 {
        return true;
    }

    if !key.0.starts_with("web.") {
        return false;
    }

    let host = |args: &str| -> Option<String> {
        let v: serde_json::Value = serde_json::from_str(args).ok()?;
        let u = v.get("url")?.as_str()?;
        url::Url::parse(u).ok()?.host_str().map(|h| h.to_string())
    };

    let Some(h) = host(&key.1) else {
        return false;
    };

    let same_site = |p: &&(String, String)| -> bool {
        p.0 == key.0
            && host(&p.1)
                .map(|o| {
                    let base = |x: &str| -> String {
                        let mut it = x.rsplit('.');
                        let t = it.next().unwrap_or("");
                        let m = it.next().unwrap_or("");
                        format!("{m}.{t}")
                    };

                    base(&o) == base(&h)
                })
                .unwrap_or(false)
    };

    recent.iter().rev().take_while(same_site).count() >= 2
}

pub trait ChatSink: Send + Sync {
    fn emit(&self, ev: StreamEvent);

    fn term_chan(&self) -> Option<&Channel<StreamEvent>> {
        None
    }
}

impl ChatSink for Channel<StreamEvent> {
    fn emit(&self, ev: StreamEvent) {
        let _ = self.send(ev);
    }

    fn term_chan(&self) -> Option<&Channel<StreamEvent>> {
        Some(self)
    }
}

pub struct NullSink;

impl ChatSink for NullSink {
    fn emit(&self, _ev: StreamEvent) {}
}

async fn ask_approval(
    gw: &Gateway,
    sink: &dyn ChatSink,
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

async fn run_skill_reflection<R: tauri::Runtime>(app: &AppHandle<R>, session_id: &str) {
    let gw = app.state::<Gateway>();

    let req: ChatReq = {
        let Ok(conn) = gw.conn.lock() else { return };
        let Ok(mut p) = project(&conn, session_id) else {
            return;
        };

        if p.model_id.is_none() {
            match auto_model(&conn) {
                Ok(m) => p.model_id = Some(m),
                Err(_) => return,
            }
        }

        let mut r = p.chat_req();
        attach_shots(&mut r.msgs);
        r.msgs.push(WireMsg {
            role: "user".into(),
            content: SKILL_NUDGE.into(),
            ..Default::default()
        });
        r
    };

    let null_chan = Channel::<StreamEvent>::new(|_| Ok(()));

    let Ok(stats) = router::stream_run(&gw, req, &null_chan).await else {
        return;
    };

    let base_text = sanitize_tags(&tools::normalize_actions(&stats.text));
    let execs = tools::build_executions(&base_text, &stats.tool_calls, 0);

    for e in execs {
        if e.tool != "skill.create" {
            continue;
        }

        let _ =
            tools::recover::exec_with_recovery(&gw, &e.tool, &e.args, "never", false, true, None)
                .await;
    }
}

fn esc_attr(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
}

fn terminal_block(idx: usize, cmd: &str, code: i64, out: &str, ms: u128) -> String {
    let status = if code == 0 { "ok" } else { "error" };
    let body = out.replace('&', "&amp;").replace('<', "&lt;");

    format!(
        "<terminal id=\"a{idx}\" command=\"{}\" status=\"{status}\" duration_ms=\"{ms}\">{}</terminal>",
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

    v.get("ref")
        .and_then(|r| r.as_u64())
        .map_or(format!("{tool} {what}"), |r| {
            format!("{tool} ref {r} {what}")
        })
}

fn browser_block(idx: usize, tool: &str, url: &str, what: &str) -> String {
    format!(
        "<browser-action id=\"a{idx}\" url=\"{}\" action=\"{}\">{}</browser-action>",
        esc_attr(url),
        esc_attr(tool),
        esc_attr(what)
    )
}

fn doc_field(body: &str, key: &str) -> String {
    body.lines()
        .find(|l| l.starts_with(key))
        .map(|l| l[key.len()..].trim().to_string())
        .unwrap_or_default()
}

fn doc_block(id: &str, title: &str, doctype: &str, pages: &str) -> String {
    format!(
        "<document id=\"{}\" title=\"{}\" doctype=\"{}\" pages=\"{}\" status=\"ready\" />",
        esc_attr(id),
        esc_attr(title),
        esc_attr(doctype),
        esc_attr(pages)
    )
}

fn body_url(body: &str) -> String {
    body.lines()
        .find(|l| l.starts_with("url "))
        .map(|l| l[4..].trim().to_string())
        .unwrap_or_default()
}

pub fn shot_marker(line: &str) -> Option<String> {
    let p = line
        .split("screenshot: ")
        .nth(1)?
        .split_whitespace()
        .next()?
        .trim();

    (p.ends_with(".png") && p.contains("/screenshots/shot-")).then(|| p.to_string())
}

pub fn attach_shots(msgs: &mut [WireMsg]) {
    let mut left = 2usize;

    for m in msgs.iter_mut().rev() {
        let paths: Vec<String> = m.content.lines().filter_map(shot_marker).collect();

        if paths.is_empty() || left == 0 {
            continue;
        }

        m.images = paths;
        left -= 1;
    }
}

pub fn sanitize_tags(s: &str) -> String {
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

    if let Ok(re) = regex::Regex::new(r"(?i)</?(p|div|span|command|output|think)[^>]*>") {
        t = re.replace_all(&t, "").into_owned();
    }

    if let Ok(re) = regex::Regex::new(r"(?i)<br\s*/?>") {
        t = re.replace_all(&t, "\n").into_owned();
    }

    t
}

pub async fn send<R: tauri::Runtime>(
    gw: &Gateway,
    app: &AppHandle<R>,
    session_id: &str,
    content: &str,
    sink: &dyn ChatSink,
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
                },
            )?;
        }
    }

    let null_chan = Channel::<StreamEvent>::new(|_| Ok(()));
    let model_chan: &Channel<StreamEvent> = sink.term_chan().unwrap_or(&null_chan);

    let (perm, web) = {
        let conn = gw.conn.lock().map_err(|err| err.to_string())?;
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
    let mut claim_nudges = 0usize;
    let mut forced_summary = false;
    let mut reflect_nudges = 0usize;
    let mut trunc_conts = 0usize;
    let mut empty_retries = 0usize;
    let mut finished = false;
    let mut acts_run = 0usize;
    let mut recent: Vec<(String, String)> = vec![];
    let mut turn_origin: Option<crate::tools::sandbox::Origin> = None;
    let allow_hosts: Vec<String> = {
        let conn = gw.conn.lock().map_err(|err| err.to_string())?;
        crate::gateway::store::kv_get(&conn, crate::tools::sandbox::KV_ALLOW_HOSTS)
            .and_then(|v| serde_json::from_str(&v).ok())
            .unwrap_or_default()
    };

    for _step in 0..MAX_STEPS {
        let req = {
            let conn = gw.conn.lock().map_err(|err| err.to_string())?;
            let mut p = project(&conn, session_id)?;
            if p.model_id.is_none() {
                p.model_id = Some(auto_model(&conn)?);
            }

            let mut r = p.chat_req();
            attach_shots(&mut r.msgs);

            if let Some(n) = nudge.take() {
                r.msgs.push(WireMsg {
                    role: "user".into(),
                    content: n,
                    ..Default::default()
                });
            }

            r.prefix_hash = Some(p.prefix_hash);
            r
        };

        let stats = router::stream_run(gw, req, model_chan).await?;
        tok_in_sum += stats.tok_in;

        if stats.text.trim().is_empty() && stats.tool_calls.is_empty() {
            if empty_retries < 1 {
                empty_retries += 1;
                nudge = Some(EMPTY_CONT.into());
                continue;
            }

            return Err("model returned an empty reply — try again".into());
        }

        let normalized = sanitize_tags(&tools::normalize_actions(&stats.text));
        let (closed, base_text) = tools::split_commit(&normalized);
        let mut pending = tools::build_executions(&base_text, &stats.tool_calls, act_base);

        let done = pending.is_empty();
        let text = base_text.clone();

        let calls_json = if stats.tool_calls.is_empty() {
            None
        } else {
            serde_json::to_string(&stats.tool_calls).ok()
        };

        let asst = {
            let conn = gw.conn.lock().map_err(|err| err.to_string())?;
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
                    tool_calls: calls_json,
                    tool_call_id: None,
                },
            )?
        };

        let mut trunc_overflow = false;
        if stats.truncated {
            trunc_conts += 1;

            if trunc_conts > MAX_TRUNC_CONTS {
                if done {
                    sink.emit(StreamEvent::Notice {
                        msg: "the model's reply was cut off at its output limit twice — \
                              partial work above is saved; send 'continue' to resume"
                            .into(),
                    });

                    if let Ok(conn) = gw.conn.lock() {
                        let _ = store::mark_final(&conn, &asst.id);
                    }

                    finished = true;
                    break;
                }
                trunc_overflow = true;
            } else {
                nudge = Some(TRUNC_CONT.into());

                if done {
                    continue;
                }
            }
        } else if done {
            let orphaned = tools::has_orphaned_action_block(&base_text);

            if !closed || fakes_output(&text) || has_faux_sandbox(&text) || orphaned {
                if claim_nudges < MAX_CLAIM_NUDGES {
                    claim_nudges += 1;
                    nudge = Some(NUDGE.into());
                    continue;
                }

                if !forced_summary {
                    forced_summary = true;
                    nudge = Some(SUMMARY_DEMAND.into());
                    continue;
                }

                sink.emit(StreamEvent::Notice {
                    msg: "the reply described an action but none ran — partial work \
                          above is saved; send 'continue' to let it retry"
                        .into(),
                });

                if let Ok(conn) = gw.conn.lock() {
                    let _ = conn.execute(
                        "UPDATE messages SET content = content || ?2 WHERE id = ?1",
                        params![
                            &asst.id,
                            "\n<warning severity=\"medium\">the reply described actions \
                             that never ran — nothing after the last tool-result was \
                             executed; send 'continue' to let it retry</warning>"
                        ],
                    );
                    let _ = store::mark_final(&conn, &asst.id);
                }

                finished = true;
                break;
            }

            if acts_run >= HARD_STEPS {
                let app2 = app.clone();
                let sid = session_id.to_string();
                tauri::async_runtime::spawn(async move {
                    run_skill_reflection(&app2, &sid).await;
                });
            }

            let reflect_on: bool = gw
                .conn
                .lock()
                .ok()
                .and_then(|conn| {
                    conn.query_row(
                        "SELECT reflect FROM sessions WHERE id = ?1",
                        params![session_id],
                        |r| r.get::<_, i64>(0),
                    )
                    .ok()
                })
                .map(|v| v != 0)
                .unwrap_or(false);

            if should_reflect(reflect_on, acts_run, reflect_nudges) {
                match run_reflection_check(gw, session_id, &text).await {
                    Ok(None) => {
                        if let Ok(conn) = gw.conn.lock() {
                            let _ = conn.execute(
                                "UPDATE messages SET content = content || ?2 WHERE id = ?1",
                                params![&asst.id, format!("\n{}", check_block(true))],
                            );
                        }
                    }
                    Ok(Some(instruction)) => {
                        reflect_nudges += 1;

                        if let Ok(conn) = gw.conn.lock() {
                            let _ = conn.execute(
                                "UPDATE messages SET content = content || ?2 WHERE id = ?1",
                                params![&asst.id, format!("\n{}", check_block(false))],
                            );
                        }

                        nudge = Some(instruction);
                        continue;
                    }
                    Err(_) => {}
                }
            }

            {
                let app3 = app.clone();
                tauri::async_runtime::spawn(async move {
                    let gw = app3.state::<Gateway>();
                    crate::learning::learn_pending(&gw).await;
                });
            }

            if let Ok(conn) = gw.conn.lock() {
                let _ = store::mark_final(&conn, &asst.id);
            }

            finished = true;
            break;
        }

        sink.emit(StreamEvent::Step);

        let mut edits: Vec<(usize, usize, String)> = vec![];
        let mut append_blocks: Vec<String> = vec![];
        let mut shown_candidates: Vec<(String, &'static str, String)> = vec![];

        for exec in pending.iter_mut() {
            let idx: usize = exec
                .id
                .strip_prefix('a')
                .and_then(|n| n.parse().ok())
                .unwrap_or(act_base);
            if idx >= act_base {
                act_base = idx + 1;
            }

            let is_term = exec.is_terminal_tool();
            let is_browser = exec.is_browser_tool();

            let args_v: serde_json::Value =
                serde_json::from_str(&exec.args).unwrap_or(serde_json::Value::Null);

            let cmd = if is_term {
                args_v["command"].as_str().unwrap_or_default().to_string()
            } else {
                String::new()
            };

            let pre_failed = exec.status.is_terminal();
            let mut denied = false;
            let code: i64;

            if pre_failed {
                code = -1;
            } else {
                if exec.tool_call_id.is_some() {
                    sink.emit(StreamEvent::Delta {
                        text: format!("<action tool=\"{}\">{}</action>", exec.tool, exec.args),
                    });
                }

                exec.begin();

                let sensitive = is_browser && tools::browser::sensitive(&exec.tool, &args_v).await;
                let needs_ask = (perm == "ask" && tools::is_mutating(&exec.tool)) || sensitive;

                let key = (exec.tool.clone(), exec.args.clone());
                let looped = repeated(&recent, &key);
                recent.push(key);

                let mut allow = !needs_ask && !looped;

                if !allow {
                    let what = if is_browser {
                        browser_what(&exec.tool, &args_v, false)
                    } else if exec.tool == "doc.create" {
                        args_v
                            .get("name")
                            .and_then(|v| v.as_str())
                            .map(|s| format!("doc.create {}", s))
                            .unwrap_or_else(|| "doc.create".into())
                    } else {
                        cmd.clone()
                    };

                    let reply = ask_approval(gw, sink, &approval_id(), idx as u32, &what).await;
                    allow = reply.allow;
                    denied = !allow;

                    if allow && !exec.is_browser_tool() {
                        if let Some(edited) = reply.args {
                            exec.args = edited;
                        }
                    }

                    if denied && is_term {
                        sink.emit(StreamEvent::TermEnd {
                            idx: idx as u32,
                            code: DENIED_CODE,
                        });
                    }
                }

                if looped {
                    exec.fail(
                        "same action 3 times without visible progress — change approach or ask the user"
                            .to_string(),
                    );
                    code = -1;
                } else if denied {
                    exec.cancel("action denied by user".to_string());
                    code = DENIED_CODE;
                } else {
                    let t0 = std::time::Instant::now();
                    let outcome = tools::recover::exec_with_recovery(
                        gw,
                        &exec.tool,
                        &exec.args,
                        &perm,
                        web,
                        allow,
                        sink.term_chan().map(|c| (c, idx as u32)),
                    )
                    .await;
                    exec.elapsed_ms = t0.elapsed().as_millis();
                    match outcome.result {
                        Ok(t) => {
                            code = exit_of(&t);
                            exec.succeed(t);
                        }
                        Err(err) => {
                            if exec.is_browser_tool()
                                && err.contains("Chrome is not connected to Argus")
                            {
                                sink.emit(StreamEvent::Notice {
                                    msg: "the agent needs your real Chrome once: open \
                                        chrome://extensions, enable Developer mode, click \
                                        Load unpacked and pick the Argus extension folder, \
                                        then tell it to try again"
                                        .into(),
                                });
                            }

                            exec.fail(err);
                            code = -1;
                        }
                    }
                }
            }

            debug_assert!(
                exec.status.is_terminal(),
                "tool execution must end terminal: {}",
                exec.tool
            );
            let status = exec.result_status();
            let body = exec.result_body().to_string();
            if is_browser {
                shown_candidates.push((exec.args.clone(), status, body.clone()));
            }
            let msg = exec.to_tool_result(RESULT_CLIP);

            {
                let conn = gw.conn.lock().map_err(|err| err.to_string())?;
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
                        tool_calls: None,
                        tool_call_id: exec.tool_call_id.clone(),
                    },
                )?;
            }

            if is_term {
                let terminal_failed = exec.status == tools::ToolStatus::Failed;
                if status == "ok" || terminal_failed {
                    let term_code = if denied { DENIED_CODE } else { code };
                    sink.emit(StreamEvent::TermEnd {
                        idx: idx as u32,
                        code: term_code,
                    });
                }

                let out = if denied || exec.status == tools::ToolStatus::Cancelled {
                    "command denied by user".to_string()
                } else {
                    body.strip_prefix("exit ")
                        .and_then(|r| r.split_once('\n'))
                        .map(|(_, o)| o.to_string())
                        .unwrap_or_else(|| body.clone())
                };

                let blk = terminal_block(idx, &cmd, code, &out, exec.elapsed_ms);
                match (exec.start, exec.end) {
                    (Some(s), Some(e)) => edits.push((s, e, blk)),
                    _ => append_blocks.push(blk),
                }
            }

            if is_browser {
                let url = args_v["url"]
                    .as_str()
                    .map(Into::into)
                    .unwrap_or_else(|| body_url(&body));

                let blk = browser_block(
                    idx,
                    &exec.tool,
                    &url,
                    &browser_what(&exec.tool, &args_v, true),
                );
                match (exec.start, exec.end) {
                    (Some(s), Some(e)) => edits.push((s, e, blk)),
                    _ => append_blocks.push(blk),
                }
            }

            if exec.tool == "doc.create" && status == "ok" {
                let id = doc_field(&body, "id=");
                let name = doc_field(&body, "name=");
                let ext = doc_field(&body, "ext=");
                let pages = doc_field(&body, "pages=");
                let title = if name.is_empty() {
                    "Untitled document".into()
                } else {
                    name
                };
                let blk = doc_block(&id, &title, &ext, &pages);
                match (exec.start, exec.end) {
                    (Some(s), Some(e)) => edits.push((s, e, blk)),
                    _ => append_blocks.push(blk),
                }
            }

            if exec.tool == "code.run" {
                let cmd = args_v
                    .get("command")
                    .and_then(|v| v.as_str())
                    .unwrap_or("command");
                let blk = crate::tools::sandbox::record_block(
                    cmd,
                    crate::tools::sandbox::Profile::Restricted.as_str(),
                    &crate::tools::sandbox::origin_label(turn_origin.as_ref(), &allow_hosts),
                    status,
                    &body,
                );
                match (exec.start, exec.end) {
                    (Some(s), Some(e)) => edits.push((s, e, blk)),
                    _ => append_blocks.push(blk),
                }
            }

            if exec.tool != "code.run" {
                if let Some(o) = crate::tools::sandbox::origin_of_tool(&exec.tool, &exec.args) {
                    turn_origin = Some(o);
                }
            }

            if exec.tool_call_id.is_some() && !is_term && !is_browser && exec.tool != "doc.create" {
                append_blocks.push(format!(
                    "<action tool=\"{}\">{}</action>",
                    exec.tool, exec.args
                ));
            }
        }

        if !edits.is_empty() || !append_blocks.is_empty() {
            let mut updated = text.clone();

            for (s, end, blk) in edits.into_iter().rev() {
                if s <= end
                    && end <= updated.len()
                    && updated.is_char_boundary(s)
                    && updated.is_char_boundary(end)
                {
                    updated.replace_range(s..end, &blk);
                }
            }
            for blk in append_blocks {
                if !updated.is_empty() && !updated.ends_with('\n') {
                    updated.push('\n');
                }
                updated.push_str(&blk);
            }

            let conn = gw.conn.lock().map_err(|err| err.to_string())?;
            conn.execute(
                "UPDATE messages SET content = ?2 WHERE id = ?1",
                params![&asst.id, &updated],
            )
            .map_err(|err| err.to_string())?;
        }

        for (args_json, status, body) in shown_candidates {
            if status != "ok" {
                continue;
            }
            let Ok(args_v) = serde_json::from_str::<serde_json::Value>(&args_json) else {
                continue;
            };
            if let Some(gen) = tools::browser::shown_gen_in(&body) {
                tools::browser::note_shown(&args_v, gen).await;
            }
        }

        acts_run += pending.len();

        if trunc_overflow {
            sink.emit(StreamEvent::Notice {
                msg: "the model's reply was cut off at its output limit twice — \
                      partial work above is saved; send 'continue' to resume"
                    .into(),
            });
            finished = true;
            break;
        }
    }

    if !finished {
        sink.emit(StreamEvent::Notice {
            msg: format!(
                "paused mid-task after {MAX_STEPS} steps — everything above is saved; \
                 send 'continue' to resume"
            ),
        });
    }

    {
        let tools_seen: Vec<String> = recent.iter().map(|(t, _)| t.clone()).collect();
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
        compressor::check(&conn, session_id)
            .map(|s| s.needs_compact)
            .unwrap_or(false)
    };

    if needs {
        if compressor::compact(gw, session_id).await.is_err() {}
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
        let temp = clean_title(content).unwrap_or_else(|| DEFAULT_TITLE.into());

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
            if generate_title(gw.inner(), &sid, &user_text).await.is_err() {}
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
    on_event: Channel<StreamEvent>,
) -> Result<(), String> {
    let notify = std::sync::Arc::new(tokio::sync::Notify::new());

    if let Ok(mut tasks) = gw.tasks.lock() {
        tasks.insert(session_id.clone(), notify.clone());
    }

    let out = tokio::select! {
        _ = notify.notified() => Err("stopped".into()),
        out = crate::tools::shell::CANCEL.scope(notify.clone(), crate::tools::notepad::SESSION_ID.scope(Some(session_id.clone()), send(&gw, &app, &session_id, &content, &on_event, "user"))) => out,
    };

    if let Ok(mut tasks) = gw.tasks.lock() {
        tasks.remove(&session_id);
    }

    out
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
