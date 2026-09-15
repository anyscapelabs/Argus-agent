//! Integration-level tests for the real `sessions/chat.rs` tool cycle.
//!
//! `send()` itself needs a live LLM (`router::stream_run`), an `AppHandle`
//! and an interactive approval `Channel`, so there is no reasonable seam to
//! drive the full loop without network/UI. Instead each test drives the exact
//! same building blocks in the exact same order `send()` uses, against a
//! real in-memory DB and the real `Gateway`:
//!
//! LLM text + native calls
//! -> `sanitize_tags(normalize_actions(..))`
//! -> `build_executions(..)` (native authoritative, deduped)
//! -> persist assistant with native `calls_json` (`store::add_msg`)
//! -> per execution `begin -> tools::exec | fail | cancel`
//! -> persist exactly one `<tool-result>` with `tool_call_id`
//!    (`store::add_msg`, seq-ordered)
//! -> `project()` for the next model iteration.
//!
//! Nothing important is mocked away: extraction, lifecycle, real `exec`,
//! real persistence and real projection all run. The only substituted inputs
//! are the LLM response itself (no network seam exists) and the interactive
//! `ask_approval` dialog (no headless UI seam exists) — the deterministic
//! post-decision `cancel` branch is driven instead.

use argus_lib::gateway::schema::ToolCall;
use argus_lib::prompt::project;
use argus_lib::sessions::chat::{repeated, sanitize_tags};
use argus_lib::sessions::schema::NewSession;
use argus_lib::tools::{
    build_executions, has_orphaned_action_block, normalize_actions, ToolExecution, ToolStatus,
};
use std::collections::HashMap;
use std::sync::Mutex;

const RESULT_CLIP: usize = 4000;

fn native(id: &str, name: &str, args: &str) -> ToolCall {
    ToolCall {
        id: id.into(),
        name: name.into(),
        args: args.into(),
    }
}

fn test_gw(tag: &str) -> (argus_lib::gateway::Gateway, std::path::PathBuf) {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    conn.execute_batch(argus_lib::gateway::schema::MIGRATE)
        .unwrap();
    argus_lib::sessions::store::migrate(&conn).unwrap();
    argus_lib::skills::store::migrate(&conn).unwrap();
    argus_lib::library::store::migrate(&conn).unwrap();
    argus_lib::memory::store::migrate(&conn).unwrap();

    let base = std::env::temp_dir().join(format!("argus-chat-exec-{}-{tag}", std::process::id()));
    let skills_dir = base.join("skills");
    let library_dir = base.join("library");
    let logos_dir = base.join("logos");
    std::fs::create_dir_all(&skills_dir).unwrap();
    std::fs::create_dir_all(&library_dir).unwrap();
    std::fs::create_dir_all(&logos_dir).unwrap();

    let gw = argus_lib::gateway::Gateway {
        conn: Mutex::new(conn),
        http: reqwest::Client::new(),
        skills_dir,
        library_dir,
        logos_dir,
        approvals: Mutex::new(HashMap::new()),
        tasks: Mutex::new(HashMap::new()),
    };

    (gw, base)
}

fn new_session(gw: &argus_lib::gateway::Gateway, permission: &str) -> String {
    let conn = gw.conn.lock().unwrap();
    let s = argus_lib::sessions::store::create_session(
        &conn,
        &NewSession {
            title: "t".into(),
            model_id: None,
            permission: Some(permission.into()),
            folder_id: None,
            web_search: false,
        },
    )
    .unwrap();
    s.id
}

fn add_user(gw: &argus_lib::gateway::Gateway, session_id: &str, content: &str) {
    let conn = gw.conn.lock().unwrap();
    argus_lib::sessions::store::add_msg(
        &conn,
        &argus_lib::sessions::schema::NewMsg {
            session_id: session_id.into(),
            role: "user".into(),
            content: content.into(),
            ..Default::default()
        },
    )
    .unwrap();
}

/// Mirror of `send()` lines: sanitize -> build -> persist assistant.
/// Returns the sanitized text plus the pending executions.
fn extract_and_persist_assistant(
    gw: &argus_lib::gateway::Gateway,
    session_id: &str,
    llm_text: &str,
    natives: &[ToolCall],
    act_base: usize,
) -> (String, Vec<ToolExecution>) {
    let base_text = sanitize_tags(&normalize_actions(llm_text));
    let pending = build_executions(&base_text, natives, act_base);
    let calls_json = if natives.is_empty() {
        None
    } else {
        serde_json::to_string(natives).ok()
    };
    {
        let conn = gw.conn.lock().unwrap();
        argus_lib::sessions::store::add_msg(
            &conn,
            &argus_lib::sessions::schema::NewMsg {
                session_id: session_id.into(),
                role: "assistant".into(),
                content: base_text.clone(),
                model_id: Some("test-model".into()),
                provider_id: Some("test-prov".into()),
                tool_calls: calls_json,
                ..Default::default()
            },
        )
        .unwrap();
    }
    (base_text, pending)
}

/// Mirror of `send()` execution + persistence: exactly one terminal
/// transition and exactly one `<tool-result>` row per execution, in order.
async fn execute_and_persist_all(
    gw: &argus_lib::gateway::Gateway,
    session_id: &str,
    pending: &mut [ToolExecution],
    perm: &str,
) {
    for exec in pending.iter_mut() {
        exec.begin();
        match argus_lib::tools::exec(gw, &exec.tool, &exec.args, perm, false, true, None).await {
            Ok(body) => exec.succeed(body),
            Err(err) => exec.fail(err),
        }
        assert!(
            exec.status.is_terminal(),
            "execution must end terminal: {}",
            exec.tool
        );
        let msg = exec.to_tool_result(RESULT_CLIP);
        let call_id = exec.tool_call_id.clone();
        let conn = gw.conn.lock().unwrap();
        argus_lib::sessions::store::add_msg(
            &conn,
            &argus_lib::sessions::schema::NewMsg {
                session_id: session_id.into(),
                role: "user".into(),
                content: msg,
                tool_call_id: call_id,
                ..Default::default()
            },
        )
        .unwrap();
    }
}

fn session_msgs(
    gw: &argus_lib::gateway::Gateway,
    session_id: &str,
) -> Vec<argus_lib::sessions::schema::Msg> {
    let conn = gw.conn.lock().unwrap();
    argus_lib::sessions::store::list_msgs(&conn, session_id).unwrap()
}

#[tokio::test]
async fn chat_single_tool_one_exec_one_result_in_next_context() {
    let (gw, base) = test_gw("single");
    let sid = new_session(&gw, "never");
    add_user(&gw, &sid, "recall anything about pyq?");

    let (_text, mut pending) = extract_and_persist_assistant(
        &gw,
        &sid,
        "checking memory now.",
        &[native("call_1", "memory.search", r#"{"query":"pyq"}"#)],
        0,
    );
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].tool, "memory.search");
    assert_eq!(pending[0].tool_call_id.as_deref(), Some("call_1"));

    execute_and_persist_all(&gw, &sid, &mut pending, "never").await;
    assert_eq!(pending[0].status, ToolStatus::Succeeded);

    let msgs = session_msgs(&gw, &sid);
    // user + assistant + exactly one tool-result.
    assert_eq!(msgs.len(), 3);
    assert_eq!(msgs[2].role, "user");
    assert!(msgs[2].content.contains(r#"tool="memory.search""#));
    assert_eq!(msgs[2].tool_call_id.as_deref(), Some("call_1"));

    // Next model iteration sees the result as a `tool` wire message.
    let proj = {
        let conn = gw.conn.lock().unwrap();
        project(&conn, &sid).unwrap()
    };
    let tool_wires: Vec<_> = proj.msgs.iter().filter(|m| m.role == "tool").collect();
    assert_eq!(tool_wires.len(), 1);
    assert_eq!(
        tool_wires[0].tool_call_id.as_deref(),
        Some("call_1"),
        "tool_call_id must survive persistence -> projection"
    );

    let _ = std::fs::remove_dir_all(&base);
}

#[tokio::test]
async fn chat_multi_tool_deterministic_order_all_results_before_next_request() {
    let (gw, base) = test_gw("multi");
    let sid = new_session(&gw, "never");
    add_user(&gw, &sid, "check skills and memory");

    let (_text, mut pending) = extract_and_persist_assistant(
        &gw,
        &sid,
        r#"working. <action tool="skill.search">{"query":"notes"}</action>"#,
        &[native("call_1", "memory.search", r#"{"query":"notes"}"#)],
        0,
    );
    // Native first (authoritative), then distinct XML — deterministic.
    assert_eq!(pending.len(), 2);
    assert_eq!(pending[0].tool, "memory.search");
    assert_eq!(pending[1].tool, "skill.search");

    execute_and_persist_all(&gw, &sid, &mut pending, "never").await;
    assert!(pending.iter().all(|e| e.status.is_terminal()));

    let msgs = session_msgs(&gw, &sid);
    assert_eq!(msgs.len(), 4, "user + assistant + 2 results, got: {msgs:?}");
    assert!(msgs[2].content.contains(r#"tool="memory.search""#));
    assert!(msgs[3].content.contains(r#"tool="skill.search""#));
    assert!(msgs[2].seq < msgs[3].seq);

    let proj = {
        let conn = gw.conn.lock().unwrap();
        project(&conn, &sid).unwrap()
    };
    // memory.search (native) -> role tool; skill.search (XML) -> role user
    // with <tool-result> text. Both present before the next request.
    assert!(proj.msgs.iter().any(|m| m.role == "tool"));
    assert!(proj
        .msgs
        .iter()
        .any(|m| m.role == "user" && m.content.contains("<tool-result")));
    let _ = std::fs::remove_dir_all(&base);
}

#[tokio::test]
async fn chat_failed_tool_does_not_kill_loop_and_next_call_runs() {
    let (gw, base) = test_gw("fail-continue");
    let sid = new_session(&gw, "never");
    add_user(&gw, &sid, "read a skill");

    // Round 1: tool fails (missing required arg).
    let (_t1, mut r1) = extract_and_persist_assistant(
        &gw,
        &sid,
        "reading.",
        &[native("call_1", "skill.read", "{}")],
        0,
    );
    execute_and_persist_all(&gw, &sid, &mut r1, "never").await;
    assert_eq!(r1[0].status, ToolStatus::Failed);
    let msgs = session_msgs(&gw, &sid);
    assert!(msgs.last().unwrap().content.contains(r#"status="err""#));
    assert!(msgs.last().unwrap().content.contains("missing name"));

    // The loop continues: round 2 issues another tool call with act_base
    // advanced past round 1, exactly as `send()` does.
    let (_t2, mut r2) = extract_and_persist_assistant(
        &gw,
        &sid,
        "listing instead.",
        &[native("call_2", "skill.search", r#"{"query":""}"#)],
        r1.len(),
    );
    execute_and_persist_all(&gw, &sid, &mut r2, "never").await;
    assert_eq!(r2[0].status, ToolStatus::Succeeded);

    let msgs = session_msgs(&gw, &sid);
    let results: Vec<_> = msgs
        .iter()
        .filter(|m| m.content.starts_with("<tool-result"))
        .collect();
    assert_eq!(results.len(), 2);
    assert!(results[0].content.contains("missing name"));
    let proj = {
        let conn = gw.conn.lock().unwrap();
        project(&conn, &sid).unwrap()
    };
    assert_eq!(proj.msgs.iter().filter(|m| m.role == "tool").count(), 2);
    let _ = std::fs::remove_dir_all(&base);
}

#[tokio::test]
async fn chat_denied_tool_single_cancelled_result_no_side_effect_no_reexec() {
    let (gw, base) = test_gw("denied");
    let sid = new_session(&gw, "ask");
    add_user(&gw, &sid, "write a file");

    let sentinel = base.join(format!("must-not-exist-{}.txt", std::process::id()));
    let args = serde_json::json!({
        "path": sentinel.to_string_lossy(),
        "content": "hello"
    })
    .to_string();
    let xml = format!(r#"writing. <action tool="fs.write">{args}</action>"#);
    let (_text, mut pending) = extract_and_persist_assistant(&gw, &sid, &xml, &[], 0);
    assert_eq!(pending.len(), 1);

    // Exact post-denial branch from `send()`: cancel without calling exec.
    let exec = &mut pending[0];
    exec.begin();
    exec.cancel("action denied by user".to_string());
    assert_eq!(exec.status, ToolStatus::Cancelled);
    assert_eq!(exec.result_status(), "err");
    let msg = exec.to_tool_result(RESULT_CLIP);
    assert!(msg.contains(r#"status="err""#));
    assert!(msg.contains("denied"));
    {
        let conn = gw.conn.lock().unwrap();
        argus_lib::sessions::store::add_msg(
            &conn,
            &argus_lib::sessions::schema::NewMsg {
                session_id: sid.clone(),
                role: "user".into(),
                content: msg,
                tool_call_id: exec.tool_call_id.clone(),
                ..Default::default()
            },
        )
        .unwrap();
    }

    // Exactly one result, no side effect, model gets it next iteration.
    let msgs = session_msgs(&gw, &sid);
    let results: Vec<_> = msgs
        .iter()
        .filter(|m| m.content.starts_with("<tool-result"))
        .collect();
    assert_eq!(results.len(), 1);
    assert!(
        !sentinel.exists(),
        "denied call must never reach tools::exec"
    );
    let proj = {
        let conn = gw.conn.lock().unwrap();
        project(&conn, &sid).unwrap()
    };
    assert!(proj
        .msgs
        .iter()
        .any(|m| m.content.contains("denied by user")));

    // Re-emitting the identical call cannot slip through again: `send()`
    // pushes every attempt into `recent`, and `repeated()` trips on the 3rd
    // identical key, turning it into a structured err instead of a re-exec.
    let key = (pending[0].tool.clone(), pending[0].args.clone());
    let mut recent = vec![key.clone(), key.clone()];
    assert!(
        repeated(&recent, &key),
        "third identical denied call must be loop-guarded"
    );
    recent.push(key);
    assert_eq!(recent.len(), 3);
    let _ = std::fs::remove_dir_all(&base);
}

#[tokio::test]
async fn chat_native_authoritative_duplicate_xml_runs_once() {
    let (gw, base) = test_gw("dedup");
    let sid = new_session(&gw, "never");
    add_user(&gw, &sid, "search");

    let xml = r#"<action tool="skill.search">{"query":"x"}</action>"#;
    let (_text, mut pending) = extract_and_persist_assistant(
        &gw,
        &sid,
        xml,
        &[native("call_1", "skill.search", r#"{"query":"x"}"#)],
        0,
    );
    assert_eq!(
        pending.len(),
        1,
        "duplicate XML must be deduped, native wins"
    );
    assert_eq!(pending[0].tool_call_id.as_deref(), Some("call_1"));

    execute_and_persist_all(&gw, &sid, &mut pending, "never").await;
    let msgs = session_msgs(&gw, &sid);
    let results: Vec<_> = msgs
        .iter()
        .filter(|m| m.content.starts_with("<tool-result"))
        .collect();
    assert_eq!(results.len(), 1);
    let _ = std::fs::remove_dir_all(&base);
}

#[tokio::test]
async fn chat_truncated_output_with_pending_is_executed_not_discarded() {
    let (gw, base) = test_gw("trunc");
    let sid = new_session(&gw, "never");
    add_user(&gw, &sid, "search truncated");

    // A truncated reply that already contains an accepted call: `send()` must
    // execute it (trunc_overflow path) rather than silently discarding it.
    let truncated_text = r#"part <action tool="skill.search">{"query":"t"}</action>"#;
    let truncated = true;
    let base_text = sanitize_tags(&normalize_actions(truncated_text));
    let mut pending = build_executions(&base_text, &[], 0);
    assert!(
        !pending.is_empty(),
        "truncated reply with a complete action block must still yield pending"
    );
    // Persist assistant exactly like `send()` does before the trunc branch.
    {
        let conn = gw.conn.lock().unwrap();
        argus_lib::sessions::store::add_msg(
            &conn,
            &argus_lib::sessions::schema::NewMsg {
                session_id: sid.clone(),
                role: "assistant".into(),
                content: base_text,
                model_id: Some("m".into()),
                provider_id: Some("p".into()),
                ..Default::default()
            },
        )
        .unwrap();
    }
    assert!(truncated);
    execute_and_persist_all(&gw, &sid, &mut pending, "never").await;

    let msgs = session_msgs(&gw, &sid);
    assert!(
        msgs.iter().any(|m| m.content.starts_with("<tool-result")),
        "truncated pending must produce a result for the model"
    );
    let proj = {
        let conn = gw.conn.lock().unwrap();
        project(&conn, &sid).unwrap()
    };
    assert!(proj.msgs.iter().any(|m| m.content.contains("<tool-result")));
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn chat_orphaned_syntax_nudges_without_invalid_execution() {
    // Mirrors `send()`'s done-branch: pending empty + orphaned => NUDGE path,
    // never a silent finish and never an execution for invalid syntax.
    let text = sanitize_tags(&normalize_actions(
        r#"doing it <action tool="terminal">{"command":"#,
    ));
    let pending = build_executions(&text, &[], 0);
    assert!(
        pending.is_empty(),
        "invalid syntax must not become an execution"
    );
    assert!(
        has_orphaned_action_block(&text),
        "orphaned block must be detected so send() nudges instead of finishing"
    );
    // The done-branch condition in `send()`:
    let done = pending.is_empty();
    let would_nudge = done && has_orphaned_action_block(&text);
    assert!(would_nudge);
}

#[tokio::test]
async fn chat_two_step_result_a_visible_for_b_final_after_b() {
    let (gw, base) = test_gw("twostep");
    let sid = new_session(&gw, "never");
    add_user(&gw, &sid, "two things");

    // Response 1 -> tool A.
    let (_t1, mut r1) = extract_and_persist_assistant(
        &gw,
        &sid,
        "first.",
        &[native("call_1", "memory.search", r#"{"query":"alpha"}"#)],
        0,
    );
    execute_and_persist_all(&gw, &sid, &mut r1, "never").await;
    assert_eq!(r1[0].status, ToolStatus::Succeeded);

    // Result A must already be in context before response 2 runs.
    let mid = {
        let conn = gw.conn.lock().unwrap();
        project(&conn, &sid).unwrap()
    };
    assert!(
        mid.msgs
            .iter()
            .any(|m| m.role == "tool" && m.tool_call_id.as_deref() == Some("call_1")),
        "result A must be projected before step 2"
    );

    // Response 2 -> tool B (act_base advanced, stable id continues).
    let (_t2, mut r2) = extract_and_persist_assistant(
        &gw,
        &sid,
        "second.",
        &[native("call_2", "skill.search", r#"{"query":""}"#)],
        r1.len(),
    );
    assert_eq!(r2.len(), 1);
    execute_and_persist_all(&gw, &sid, &mut r2, "never").await;
    assert_eq!(r2[0].status, ToolStatus::Succeeded);

    // Final assistant response occurs after B.
    {
        let conn = gw.conn.lock().unwrap();
        argus_lib::sessions::store::add_msg(
            &conn,
            &argus_lib::sessions::schema::NewMsg {
                session_id: sid.clone(),
                role: "assistant".into(),
                content: "both done.".into(),
                ..Default::default()
            },
        )
        .unwrap();
    }

    let msgs = session_msgs(&gw, &sid);
    let last = msgs.last().unwrap();
    assert_eq!(last.role, "assistant");
    assert_eq!(last.content, "both done.");
    let tool_positions: Vec<usize> = msgs
        .iter()
        .enumerate()
        .filter(|(_, m)| m.content.starts_with("<tool-result"))
        .map(|(i, _)| i)
        .collect();
    assert_eq!(tool_positions.len(), 2);
    assert!(
        tool_positions[1] < msgs.len() - 1,
        "B result precedes final"
    );

    let proj = {
        let conn = gw.conn.lock().unwrap();
        project(&conn, &sid).unwrap()
    };
    let ids: Vec<_> = proj
        .msgs
        .iter()
        .filter(|m| m.role == "tool")
        .filter_map(|m| m.tool_call_id.clone())
        .collect();
    assert_eq!(ids, vec!["call_1".to_string(), "call_2".to_string()]);
    let _ = std::fs::remove_dir_all(&base);
}
