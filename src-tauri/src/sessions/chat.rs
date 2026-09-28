use std::sync::OnceLock;
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

const WATCH_IDLE: std::time::Duration = std::time::Duration::from_millis(150);

const WAKE_SLOTS: u32 = 240;
const WAKE_SLOT_MS: u64 = 500;
const DENIED_CODE: i64 = -2;
const MAX_CLAIM_NUDGES: usize = 2;
const MAX_TRUNC_CONTS: usize = 2;
const MAX_REFLECT_NUDGES: usize = 1;

const TITLE_SYS: &str =
    "You are the title generator for Argus, a personal AI agent the user chats with. \
Write a short session title for the user's message. Reply with only the title: \
3 to 6 words, no quotes, no trailing punctuation.";

const NUDGE: &str = "Your last reply neither ran a tool nor closed the turn. A reply ends \
one of exactly two ways: with a tool call, or with the final answer followed by \
<final/> on its own last line. If you meant to act, make the call now and end the reply \
right after it. If you cannot act, say so plainly and end with <final/> — never \
describe an action without running it.";

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

// Args that vary without changing what the call does. Retries that differ
// only here are the same attempt wearing a different timeout.
//
// `background` and `privilege` are deliberately NOT here. They are the two
// moves the tool description tells the agent to make when a foreground call
// keeps failing: background a long job, escalate a denied one. Erasing them
// makes the guard refuse the exact recovery it exists to encourage.
const THRASH_VOLATILE: &[&str] = &["timeout", "label", "wake"];

fn stable_args(args: &str) -> String {
    match serde_json::from_str::<serde_json::Value>(args) {
        Ok(serde_json::Value::Object(mut m)) => {
            for k in THRASH_VOLATILE {
                m.remove(*k);
            }
            serde_json::Value::Object(m).to_string()
        }
        _ => args.trim().to_string(),
    }
}

// Keys carry the identity of a call; values carry its bulk. `fs.write` and
// `doc.create` pair a long body with a one-token path, so comparing values
// alone says every file is the same file. Each value contributes under its
// own key, so a shared body cannot outvote the path that tells them apart.
fn arg_tokens(stable: &str) -> std::collections::HashSet<String> {
    let mut toks = std::collections::HashSet::new();

    fn walk(prefix: &str, v: &serde_json::Value, toks: &mut std::collections::HashSet<String>) {
        match v {
            serde_json::Value::String(s) => {
                for t in s.split_whitespace() {
                    toks.insert(format!("{prefix}={t}"));
                }
            }
            serde_json::Value::Number(n) => {
                toks.insert(format!("{prefix}={n}"));
            }
            serde_json::Value::Bool(b) => {
                toks.insert(format!("{prefix}={b}"));
            }
            serde_json::Value::Array(a) => {
                for (i, x) in a.iter().enumerate() {
                    walk(&format!("{prefix}[{i}]"), x, toks);
                }
            }
            serde_json::Value::Object(m) => {
                for (k, x) in m {
                    walk(&format!("{prefix}.{k}"), x, toks);
                }
            }
            serde_json::Value::Null => {}
        }
    }

    match serde_json::from_str::<serde_json::Value>(stable) {
        Ok(serde_json::Value::Object(m)) => {
            for (k, v) in &m {
                walk(k, v, &mut toks);
            }
        }
        Ok(v) => walk("", &v, &mut toks),
        Err(_) => {
            toks.extend(stable.split_whitespace().map(str::to_string));
        }
    }
    toks
}

// No comparable tokens means no evidence, so two un-informative calls are
// treated as different. Scoring emptiness as maximal similarity inverts the
// safe direction: it blocked `{"background":true}` against `false`.
fn jaccard(a: &std::collections::HashSet<String>, b: &std::collections::HashSet<String>) -> f64 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let inter = a.intersection(b).count() as f64;
    let union = a.union(b).count() as f64;
    if union == 0.0 {
        0.0
    } else {
        inter / union
    }
}

// The short argument that says *which* thing this call is about. A long body
// cannot outvote these: `fs.write` on three different files with the same
// content is three different files.
const IDENTITY_KEYS: &[&str] = &["path", "id", "name", "url", "pattern", "query", "file"];

fn identity(stable: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(stable).ok()?;
    let m = v.as_object()?;

    for k in IDENTITY_KEYS {
        if let Some(s) = m.get(*k).and_then(|x| x.as_str()) {
            let s = s.trim();

            if !s.is_empty() {
                return Some(s.to_string());
            }
        }
    }
    None
}

/// One deliberate move that must never read as a retry of itself: the two
/// things the tool description tells the agent to do when a call keeps
/// failing. Structurally these only *add* a flag, so no similarity metric can
/// tell them from churn — they are named instead.
fn escalates(win: &[&str], proposed: &str) -> bool {
    let asked_for = |a: &str| {
        let v: serde_json::Value = match serde_json::from_str(a) {
            Ok(v) => v,
            Err(_) => return false,
        };
        v.get("background").and_then(|x| x.as_bool()) == Some(true)
            || v.get("privilege")
                .and_then(|x| x.as_str())
                .is_some_and(|p| p != "user")
    };

    asked_for(proposed) && !win.iter().any(|a| asked_for(a))
}

/// Are these two attempts the same action? Growth counts: `ls` then `ls -la`
/// is one call being refined, and refusing that is refusing the whole point of
/// the guard. Divergence does not: four attempts that each barely resemble the
/// last are four different ideas, and stopping the fourth protects the agent.
fn same_action(a: &str, b: &str) -> bool {
    if let (Some(ia), Some(ib)) = (identity(a), identity(b)) {
        return ia == ib;
    }

    let ta = arg_tokens(a);
    let tb = arg_tokens(b);

    if ta.is_empty() || tb.is_empty() {
        return false;
    }
    if ta.is_subset(&tb) || tb.is_subset(&ta) {
        return true;
    }
    jaccard(&ta, &tb) >= 0.5
}

// The semantic twin of `repeated`: same tool, failing streak, arguments that
// are equal modulo volatile keys or a chain of near-identical variants —
// `ls`, `ls -la`, `ls -la /tmp` dying the same death. Successes never trip
// it: sequential similar calls that work are real multi-step work, not churn.
// `hist`/`failed` hold completed attempts only; `key` is the one proposed.
pub fn thrashing(hist: &[(String, String)], failed: &[bool], key: &(String, String)) -> bool {
    debug_assert_eq!(
        hist.len(),
        failed.len(),
        "guard history and its outcomes must stay the same length"
    );
    if hist.len() < 3 || failed.len() < 3 {
        return false;
    }
    let win = &hist[hist.len() - 3..];
    let fout = &failed[failed.len() - 3..];
    if !fout.iter().all(|f| *f) {
        return false;
    }
    if win.iter().any(|(t, _)| t != &key.0) {
        return false;
    }
    let mut stable: Vec<String> = win.iter().map(|(_, a)| stable_args(a)).collect();
    stable.push(stable_args(&key.1));
    if stable.iter().all(|s| s == &stable[0]) {
        return true;
    }

    let refs: Vec<&str> = stable.iter().map(String::as_str).collect();
    if escalates(&refs[..3], refs[3]) {
        return false;
    }
    // Every step alike, and the last still recognisably the first: drift
    // compounds, so neighbours alone let four unrelated attempts through.
    refs.windows(2).all(|w| same_action(w[0], w[1])) && same_action(refs[0], refs[refs.len() - 1])
}

// A turn spends a bounded number of context tokens, the way a training run
// spends a fixed wall clock: comparable runs, and a spiral becomes a verdict
// instead of a cost. Sized off the model's own window so a 1M model is not
// throttled like a 32k one.
const BUDGET_WINDOW_FRACTION: f64 = 0.25;
const BUDGET_WARN_FRACTION: f64 = 0.7;
const BUDGET_MIN: i64 = 32_000;
const BUDGET_MAX: i64 = 200_000;

pub fn turn_budget(ctx_tokens: i64) -> i64 {
    let base = if ctx_tokens > 0 {
        ctx_tokens
    } else {
        crate::prompt::config::DEFAULT_CONTEXT
    };
    let scaled = (base as f64 * BUDGET_WINDOW_FRACTION) as i64;
    scaled.clamp(BUDGET_MIN, BUDGET_MAX)
}

// Nudge at 70% (the model can still wind up), stop at 100% (nothing left to
// spend). Enforced, never asked for politely: a warning a model ignores is
// not a budget.
pub fn budget_state(spent: i64, budget: i64) -> Budget {
    if budget <= 0 || spent < (BUDGET_WARN_FRACTION * budget as f64) as i64 {
        return Budget::Ok;
    }
    if spent < budget {
        return Budget::Warn;
    }
    Budget::Stop
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Budget {
    Ok,
    Warn,
    Stop,
}

pub trait ChatSink: Send + Sync {
    fn emit(&self, ev: StreamEvent);

    fn term_chan(&self) -> Option<&Channel<StreamEvent>> {
        None
    }

    fn detached(&self) -> bool {
        false
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

pub struct BusSink<'a> {
    pub gw: &'a Gateway,
    pub session_id: String,
}

impl ChatSink for BusSink<'_> {
    fn emit(&self, ev: StreamEvent) {
        self.gw.publish(&self.session_id, ev);
    }

    fn detached(&self) -> bool {
        !self.gw.watched(&self.session_id)
    }
}

/// A sub-agent runs in its own session but answers to the chat that spawned
/// it. Only an approval crosses into the parent, because a human has to be
/// able to answer it and the parent chat is where approvals are answered.
/// Everything else — deltas, terminal output, and above all TurnEnd, which
/// would end the parent's turn out from under it — stays in the child.
///
/// The one thing it will not do is ask when nobody is there. `detached` keys
/// off the parent being attached, so the moment that window closes the agent
/// fails closed exactly like any other unattended turn.
pub struct FanSink<'a> {
    pub gw: &'a Gateway,
    pub child_id: String,
    pub parent_id: String,
}

impl ChatSink for FanSink<'_> {
    /// Only an approval crosses into the parent, because a human has to be
    /// able to answer it and the parent chat is where approvals are answered.
    /// Everything else — deltas, terminal output, and above all TurnEnd, which
    /// would tear down the parent's own turn — stays on the child's own bus.
    fn emit(&self, ev: StreamEvent) {
        if matches!(ev, StreamEvent::Approval { .. }) {
            self.gw.publish(&self.parent_id, ev.clone());
        }

        self.gw.publish(&self.child_id, ev);
    }

    fn detached(&self) -> bool {
        !self.gw.attached(&self.parent_id)
    }
}

pub fn attr_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Put a message in a conversation without running a turn for it, and tell
/// whoever is looking at that conversation to re-read it. This is how a card
/// gets into the middle of a chat the agent is already answering.
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
                let sink = BusSink {
                    gw: gw.inner(),
                    session_id: sid.clone(),
                };

                // A turn nobody asked for still has a person reading it, or
                // the open chat. Without this the wake refuses every step
                // that needs approving, because it thinks it is alone.
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

        let _ = tools::recover::exec_with_recovery(
            app, &gw, &e.tool, &e.args, "never", false, true, None,
        )
        .await;
    }
}

pub fn esc_attr(s: &str) -> String {
    // A value can legally contain all of these; the tag cannot survive them raw.
    s.replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('\n', "&#10;")
        .replace('\r', "&#13;")
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

        // Extend, not replace: a message can carry an image the user attached
        // and a screenshot the model took, and the second is not a reason to
        // drop the first.
        m.images.extend(paths);
        left -= 1;
    }
}

// Text blocks are the legacy record format. Fresh native turns persist prose
// only and read rows instead — unless the row write failed, in which case the
// text is the only record left and must stay. Degraded turns never have rows
// worth reading. One predicate so the rule cannot drift between callers.
pub fn needs_text_blocks(style: tools::ToolCallStyle, degraded: bool, events_ok: bool) -> bool {
    style != tools::ToolCallStyle::Native || degraded || !events_ok
}

/// Record where the turn stopped, so the next one resumes instead of
/// re-deriving. The goal is the task as first asked, not the word that
/// resumed it: on a `continue` the current message says "continue", and a
/// resume claiming that is worse than no resume.
fn save_resume(gw: &Gateway, session_id: &str, actions: &[(String, bool)]) {
    let Ok(conn) = gw.conn.lock() else {
        return;
    };
    let goal = store::first_user_msg(&conn, session_id).unwrap_or_default();
    super::resume::save(&conn, session_id, &goal, actions);
}

/// What one finished execution tells the playbook. One place, so a new failure
/// mode is recorded by adding a line here rather than by remembering seven
/// scattered call sites.
///
/// Each check matches a string Argus itself wrote about its own behaviour —
/// an error message from this crate, a note the sandbox layer attached. A
/// model cannot talk its way into a lesson, because model prose never reaches
/// these comparisons.
pub fn observe_exec(
    conn: &rusqlite::Connection,
    model_id: &str,
    exec: &tools::ToolExecution,
    thrashed: bool,
) {
    observe_signals(conn, model_id, exec, thrashed)
}

fn observe_signals(
    conn: &rusqlite::Connection,
    model_id: &str,
    exec: &tools::ToolExecution,
    thrashed: bool,
) {
    use crate::playbook::store::{record, Kind};

    let err = exec.error.as_deref().unwrap_or_default();

    if err.contains(EMPTY_ARGS_ERR) {
        let _ = record(conn, Kind::EmptyArgs, model_id, None, &exec.tool);
    }

    if thrashed {
        let _ = record(conn, Kind::Thrashing, model_id, None, &exec.tool);
    }

    let host = crate::sessions::ext_install::host_id();

    if err.contains(MISSING_CWD_ERR) {
        let _ = record(conn, Kind::SandboxNoCwd, &host, None, &exec.tool);
    }

    if err.contains(crate::tools::sandbox::DENIAL_NOTE) || denial_in_output(exec) {
        let _ = record(conn, Kind::SandboxDenied, &host, None, &exec.tool);
    }
}

/// A sandbox refusal is appended after the command's own output, so it is the
/// tail of the body. Checking the tail rather than the whole body means a
/// file the agent read containing these words is not mistaken for a refusal.
///
/// The residual is honest: a command could print the exact trailing string. It
/// would cost one host lesson about a sandbox, and closing it properly needs a
/// per-run nonce shared between the sandbox and the chat loop, which is not
/// worth the coupling for a note that is already in the model's context.
fn denial_in_output(exec: &tools::ToolExecution) -> bool {
    const TAIL_MAX: usize = 200;

    let body = exec.result_body();
    let Some(at) = body.rfind(crate::tools::sandbox::DENIAL_NOTE) else {
        return false;
    };

    body.len() - at <= TAIL_MAX
}

/// Phrases this crate writes about its own behaviour. Matching on them is what
/// makes a signal unforgeable: the model cannot emit them into a place we read.
const EMPTY_ARGS_ERR: &str = "arrived with empty arguments";
const MISSING_CWD_ERR: &str = "project profile needs cwd";

/// What the resume says ran. Read from `exec.args` at record time, not from
/// the args the model proposed: a user-approved edit rewrites them, and a
/// resume that misreports the approved command is worse than no command.
fn exec_label(exec: &tools::ToolExecution, is_term: bool) -> String {
    if is_term {
        let v: serde_json::Value = serde_json::from_str(&exec.args).unwrap_or_default();
        let cmd = v["command"].as_str().unwrap_or_default().trim();

        if !cmd.is_empty() {
            return format!("terminal: {cmd}");
        }
    }

    let args: serde_json::Value = serde_json::from_str(&exec.args).unwrap_or_default();
    hint_of_args(&args, &exec.tool)
}

/// A tool plus the one argument that identifies it. Two different `fs.write`
/// calls must not read as the same action in a resume.
fn hint_of_args(args: &serde_json::Value, tool: &str) -> String {
    for k in ["command", "url", "query", "name", "path", "pattern", "id"] {
        if let Some(s) = args[k].as_str() {
            let s = s.trim();

            if !s.is_empty() {
                let head: String = s.chars().take(80).collect();
                return format!("{tool} {head}");
            }
        }
    }

    tool.to_string()
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

    // Compiled once, not per message. The `regex` crate's own docs call
    // compiling inside a function an anti-pattern: it costs microseconds to
    // milliseconds each time, and this runs on every assistant message.
    static DROP_TAGS: OnceLock<regex::Regex> = OnceLock::new();
    static BR: OnceLock<regex::Regex> = OnceLock::new();

    // `expect` rather than a `None` fallback: a pattern that fails to compile
    // is a bug in this source file, not bad input, and a silent skip would
    // leave the tags in the transcript with no signal that anything went wrong.
    let drop_tags = DROP_TAGS.get_or_init(|| {
        regex::Regex::new(r"(?i)</?(p|div|span|command|output|think)[^>]*>")
            .expect("drop-tag pattern")
    });
    t = drop_tags.replace_all(&t, "").into_owned();

    let br = BR.get_or_init(|| regex::Regex::new(r"(?i)<br\s*/?>").expect("br pattern"));
    t = br.replace_all(&t, "\n").into_owned();

    t
}

pub async fn send<R: tauri::Runtime>(
    gw: &Gateway,
    app: &AppHandle<R>,
    session_id: &str,
    content: &str,
    attachments: Option<&str>,
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
                    attachments: attachments.map(str::to_string),
                },
            )?;
        }
        // The previous turn's resume is deliberately left in place: it is the
        // whole point of a resume, and the first `project()` below is the only
        // reader. It is overwritten on every unfinished stop and cleared the
        // moment a turn finishes, so it can never describe work the model has
        // already moved past.
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
    // Outcomes of completed attempts, aligned with `recent`: a semantic retry
    // streak only means churn when every attempt in it failed.
    let mut recent_out: Vec<bool> = vec![];
    let mut turn_origin: Option<crate::tools::sandbox::Origin> = None;
    let allow_hosts: Vec<String> = {
        let conn = gw.conn.lock().map_err(|err| err.to_string())?;
        crate::gateway::store::kv_get(&conn, crate::tools::sandbox::KV_ALLOW_HOSTS)
            .and_then(|v| serde_json::from_str(&v).ok())
            .unwrap_or_default()
    };
    let turn_budget = {
        let conn = gw.conn.lock().map_err(|err| err.to_string())?;
        let row: (Option<String>, i64) = conn
            .query_row(
                "SELECT model_id, ctx_tokens FROM sessions WHERE id = ?1",
                params![session_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap_or((None, 0));
        let window = crate::prompt::config::context_window(&conn, row.0.as_deref());
        turn_budget(if row.1 > 0 { row.1 } else { window })
    };
    let mut budget_warned = false;
    // (label, succeeded) per exec this turn, in order. The resume record is
    // built from it: the loop's own account, not the model's.
    let mut turn_actions: Vec<(String, bool)> = vec![];

    for _step in 0..MAX_STEPS {
        let req = {
            let conn = gw.conn.lock().map_err(|err| err.to_string())?;
            let mut p = project(&conn, session_id, &gw.library_dir)?;
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

        // Checked before the call, not after: at Stop the turn is over and the
        // model never gets to spend what is left of the budget.
        if budget_state(tok_in_sum, turn_budget) == Budget::Stop {
            // One of three places that promise the user a 'continue' can pick
            // the work up, so all three have to leave a resume behind.
            save_resume(gw, session_id, &turn_actions);

            if let Ok(conn) = gw.conn.lock() {
                let _ = crate::playbook::store::record(
                    &conn,
                    crate::playbook::Kind::BudgetStop,
                    &crate::sessions::ext_install::host_id(),
                    Some(session_id),
                    &format!("{tok_in_sum} of {turn_budget} tokens"),
                );
            }
            if let Ok(conn) = gw.conn.lock() {
                if let Ok(last) = store::get_last_final(&conn, session_id) {
                    let _ = conn.execute(
                        "UPDATE messages SET content = content || ?2 WHERE id = ?1",
                        params![
                            &last,
                            "\n<warning severity=\"medium\">this turn hit its budget — the work \
                             above is saved; send 'continue' to pick it up in a new turn</warning>"
                        ],
                    );
                    let _ = store::mark_final(&conn, &last);
                }
            }
            sink.emit(StreamEvent::Notice {
                msg: "this turn hit its budget — the work above is saved; send 'continue' to \
                      pick it up in a new turn"
                    .into(),
            });
            finished = true;
            break;
        }

        if !budget_warned && budget_state(tok_in_sum, turn_budget) == Budget::Warn {
            // A contract break already queued its own correction and is more
            // urgent than a heads-up, so the budget warning waits one step
            // rather than overwriting it. Latched only once actually queued,
            // or it would be lost and never re-fires.
            if nudge.is_none() {
                budget_warned = true;
                nudge = Some(format!(
                    "You have used about {pct}% of this turn's budget. Finish the task now, \
                     or report what is done and what is left in one answer. Close it with \
                     <final/>.",
                    pct = (BUDGET_WARN_FRACTION * 100.0) as u32
                ));
            }
        }

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
        // Degraded replies have no API channel, so the text must execute
        // regardless of style. Otherwise the resolved style decides: native
        // prose is never executed, template text is decoded.
        let style = if stats.degraded {
            tools::ToolCallStyle::GlmXml
        } else {
            match gw.conn.lock() {
                Ok(conn) => crate::prompt::config::tool_style(&conn, Some(stats.model_id.as_str())),
                Err(_) => tools::ToolCallStyle::Native,
            }
        };
        let mut pending =
            tools::build_executions_styled(&base_text, &stats.tool_calls, act_base, style);

        // An unknown model showing the duplication signature gets classified
        // once, here. Next turn it resolves to the template style directly.
        if style == tools::ToolCallStyle::Native
            && tools::has_native_text_duplicate(&base_text, &stats.tool_calls)
        {
            if let Ok(conn) = gw.conn.lock() {
                crate::prompt::config::upgrade_tool_style(&conn, &stats.model_id);
            }
        }

        let done = pending.is_empty();
        // Native turns persist prose only: the event rows own what ran, so no
        // record markup is stored to be re-parsed later. Degraded and template
        // turns keep the text blocks — they are the only record those have.
        let text = if style == tools::ToolCallStyle::Native && !stats.degraded {
            tools::strip_actions(&base_text)
        } else {
            tools::render_actions(&base_text, &pending)
        };

        let calls_json = if stats.tool_calls.is_empty() {
            None
        } else {
            serde_json::to_string(&stats.tool_calls).ok()
        };

        // The turn told us it speaks the XML dialect by writing it. That is a
        // fact about the model, recorded where the classification happens, so
        // a lesson exists for the same turn that caused it.
        if style == tools::ToolCallStyle::GlmXml && !stats.degraded {
            if let Ok(conn) = gw.conn.lock() {
                let _ = crate::playbook::store::record(
                    &conn,
                    crate::playbook::Kind::StyleXml,
                    &stats.model_id,
                    Some(session_id),
                    "wrote a tool call as XML text",
                );
            }
        }

        // The provider refused the tool schemas. The turn still worked, via
        // the in-band format, so this is infrastructure, not a model failure.
        if stats.degraded {
            if let Ok(conn) = gw.conn.lock() {
                let _ = crate::playbook::store::record(
                    &conn,
                    crate::playbook::Kind::Degraded,
                    &crate::sessions::ext_install::host_id(),
                    Some(session_id),
                    &stats.provider_id,
                );
            }
        }

        let model_id = stats.model_id.clone();
        let (asst, said_again) = {
            let conn = gw.conn.lock().map_err(|err| err.to_string())?;
            store::add_msg_dedup(
                &conn,
                &NewMsg {
                    session_id: session_id.into(),
                    role: "assistant".into(),
                    content: text.clone(),
                    model_id: Some(model_id.clone()),
                    provider_id: Some(stats.provider_id),
                    tok_in: Some(stats.tok_in),
                    tok_out: Some(stats.tok_out),
                    tool_calls: calls_json,
                    tool_call_id: None,
                    attachments: None,
                },
            )?
        };

        let mut trunc_overflow = false;
        if stats.truncated {
            trunc_conts += 1;

            // A model told to carry on and answering the same thing has
            // nothing left to say. Asking again only buys another copy of the
            // same words, which is how one answer ends up in the chat three
            // times over.
            if trunc_conts > MAX_TRUNC_CONTS || said_again {
                if done {
                    // A model that just repeated itself was not cut off — it had
                    // nothing left to say. Blaming a limit it never hit would be
                    // a worse lie than saying nothing.
                    if !said_again {
                        sink.emit(StreamEvent::Notice {
                            msg: "the model's reply was cut off at its output limit twice — \
                                  partial work above is saved; send 'continue' to resume"
                                .into(),
                        });
                    }

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
            // Asked of what the model actually wrote, not of the text the
            // transcript shows: a fragment too broken to run is cut out of the
            // answer, and the model still has to be told to say it again.
            let orphaned = tools::has_orphaned_action_block(&stats.text);

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
                            "\n<warning severity=\"medium\">this turn described actions that \
                             never ran — the work above is saved; send 'continue' to let it \
                             retry</warning>"
                        ],
                    );
                    let _ = store::mark_final(&conn, &asst.id);
                }
                // "Send continue to retry" is a promise, so the retry has to
                // find out what was attempted.
                save_resume(gw, session_id, &turn_actions);

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

            // A finished turn closes its own resume: leaving one behind would
            // tell the next turn there is unfinished work that is not.
            if let Ok(conn) = gw.conn.lock() {
                super::resume::clear(&conn, session_id);
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
                let sid = session_id.to_string();
                let mid = model_id.clone();
                tauri::async_runtime::spawn(async move {
                    let app4 = app3.clone();
                    let gw = app4.state::<Gateway>();
                    crate::learning::learn_pending(&gw).await;

                    // Curating is a lookup and an upsert per kind, with the
                    // sentences fixed in code — no model call, so it cannot
                    // stall a turn or cost anything.
                    let conn = gw.conn.lock().ok();
                    if let Some(conn) = conn {
                        let _ = crate::playbook::curate(&conn, &mid, Some(&sid));
                        let _ = crate::playbook::curate(
                            &conn,
                            &crate::sessions::ext_install::host_id(),
                            Some(&sid),
                        );
                    }
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

        let mut events_ok = true;

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
            // Set on the gate path below, read by the single observation site
            // after it. A pre-failed exec never trips the guard, so it starts
            // false rather than reading a stale value from the last exec.
            let mut thrashed = false;
            let code: i64;

            if pre_failed {
                code = -1;
                // Recorded here because the gate below is skipped, but the
                // outcome itself is pushed once for every exec further down,
                // so both histories stay aligned and failure-first.
                recent.push((exec.tool.clone(), exec.args.clone()));
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
                thrashed = thrashing(&recent, &recent_out, &key);
                recent.push(key);

                // A guard trip is not an approval question, so it never opens
                // the Run/Deny card: showing it and then failing the call
                // anyway asks the user to approve something already refused.
                // The user is the escape hatch, and the guard is advisory.
                let mut allow = !needs_ask && !looped && !thrashed;
                let needs_ask = needs_ask && !looped && !thrashed;

                if !allow && (looped || thrashed) {
                    sink.emit(StreamEvent::Notice {
                        msg: if thrashed {
                            "skipped a step that retried the same failing approach — say what to \
                             try instead, or run it yourself"
                        } else {
                            "skipped a step that repeats the same call — say what to try \
                             instead, or run it yourself"
                        }
                        .into(),
                    });
                }

                if needs_ask {
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

                    let mut edited: Option<String> = None;

                    if sink.detached() {
                        sink.emit(StreamEvent::Notice {
                            msg: format!(
                                "skipped a step that needs your approval ({what}) — nothing was \
                                 listening, so it was refused rather than guessed at. Open the \
                                 session and ask again, or set it to never"
                            ),
                        });
                        allow = false;
                        denied = true;
                    } else {
                        let reply = ask_approval(gw, sink, &approval_id(), idx as u32, &what).await;
                        edited = reply.args;
                        allow = reply.allow;
                        denied = !allow;
                    }

                    if allow && !exec.is_browser_tool() {
                        if let Some(args) = edited {
                            exec.args = args;
                        }
                    }

                    if denied && is_term {
                        sink.emit(StreamEvent::TermEnd {
                            idx: idx as u32,
                            code: DENIED_CODE,
                        });
                    }
                }

                if looped || thrashed {
                    exec.fail(
                        if thrashed {
                            "retried the same approach without progress — change approach or ask the user"
                        } else {
                            "same action 3 times without visible progress — change approach or ask the user"
                        }
                        .to_string(),
                    );
                    code = -1;
                } else if denied {
                    exec.cancel("action denied by user".to_string());
                    code = DENIED_CODE;
                } else {
                    let t0 = std::time::Instant::now();
                    let outcome = tools::recover::exec_with_recovery(
                        app,
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
            // Outcome for the semantic guard. One push per exec, on every path
            // including pre-failed ones that never ran: the caller pushes to
            // `recent` unconditionally too, and the two must stay the same
            // length for the window slices to mean anything.
            let exec_failed = exec.status == tools::ToolStatus::Failed
                || exec.status == tools::ToolStatus::Cancelled;
            recent_out.push(exec_failed);
            turn_actions.push((exec_label(exec, is_term), !exec_failed));

            // A failure is the one moment worth learning from, so it is
            // observed here rather than at each site that could fail. A
            // dropped signal costs a lesson; a spurious one costs prompt
            // budget, and neither is worth a turn's outcome.
            if exec_failed {
                if let Ok(conn) = gw.conn.lock() {
                    observe_exec(&conn, &stats.model_id, exec, thrashed);
                }
            }

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
                        attachments: None,
                    },
                )?;
            }

            // The structured twin of the text block below. Same execution, so
            // the card, the history, and the audit can never disagree. A lost
            // row falls back to text (the turn must never fail over logging)
            // and is counted in connector_logs under service `sessions`.
            let ev = crate::sessions::events::from_execution(exec, &asst.id, session_id);
            let ev_err = match gw.conn.lock() {
                Ok(conn) => store::add_event(&conn, &ev).err(),
                Err(err) => Some(err.to_string()),
            };
            if let Some(err) = ev_err {
                events_ok = false;
                crate::connectors::log::event(
                    "sessions",
                    "tool_event_write",
                    &format!("{}: {err}", exec.tool),
                    "err",
                );
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

        // Native turns keep prose only: events own the records (written
        // above), so splicing text blocks would resurrect the markup the
        // structured path exists to delete. A lost event row falls back to
        // text so the work stays visible. Degraded turns have no events
        // worth reading, so their text blocks stay.
        if (!edits.is_empty() || !append_blocks.is_empty())
            && needs_text_blocks(style, stats.degraded, events_ok)
        {
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
            save_resume(gw, session_id, &turn_actions);
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
        save_resume(gw, session_id, &turn_actions);
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
        compressor::check(&conn, session_id, &gw.library_dir)
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

    sink.emit(StreamEvent::TurnEnd {
        session_id: session_id.into(),
    });

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

    let sink = BusSink {
        gw: gw.inner(),
        session_id: session_id.clone(),
    };

    let out = tokio::select! {
        _ = notify.notified() => Err("stopped".into()),
        out = crate::tools::shell::CANCEL.scope(notify.clone(), crate::tools::notepad::SESSION_ID.scope(Some(session_id.clone()), send(&gw, &app, &session_id, &content, attachments.as_deref(), &sink, "user"))) => out,
    };

    let _ = fwd.await;

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
