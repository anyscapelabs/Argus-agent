use argus_lib::gateway::schema::ToolCall;
use argus_lib::tools::{build_executions, has_orphaned_action_block, ToolStatus};
use std::collections::HashMap;
use std::sync::Mutex;

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

    let base = std::env::temp_dir().join(format!("argus-tool-exec-{}-{tag}", std::process::id()));
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

#[test]
fn lifecycle_enforces_single_terminal_state() {
    let mut e = build_executions(
        r#"<action tool="grep">{"pattern":"x","path":"."}</action>"#,
        &[],
        0,
    )
    .pop()
    .expect("one execution");
    assert_eq!(e.status, ToolStatus::Created);
    assert!(!e.status.is_terminal());

    e.begin();
    assert_eq!(e.status, ToolStatus::Executing);

    e.succeed("ok-body".into());
    assert_eq!(e.status, ToolStatus::Succeeded);
    assert!(e.status.is_terminal());
    assert_eq!(e.result_status(), "ok");
    let msg = e.to_tool_result(4000);
    assert!(msg.contains(r#"tool="grep" status="ok""#));
    assert!(msg.contains("ok-body"));

    let mut f = build_executions(
        r#"<action tool="grep">{"pattern":"x","path":"."}</action>"#,
        &[],
        0,
    )
    .pop()
    .unwrap();
    f.begin();
    f.fail("boom".into());
    assert_eq!(f.result_status(), "err");
    assert!(f.to_tool_result(4000).contains("boom"));

    let mut c = build_executions(
        r#"<action tool="terminal">{"command":"ls"}</action>"#,
        &[],
        0,
    )
    .pop()
    .unwrap();
    c.begin();
    c.cancel("action denied by user".into());
    assert_eq!(c.status, ToolStatus::Cancelled);
    assert_eq!(c.result_status(), "err");
    assert!(c.to_tool_result(4000).contains("denied"));
}

#[test]
fn native_calls_are_authoritative_and_ordered() {
    let text = r#"check <action tool="skill.search">{"query":"x"}</action> done"#;
    let natives = vec![
        native("call_1", "skill.search", r#"{"query":"x"}"#),
        native("call_2", "memory.search", r#"{"query":"y"}"#),
    ];
    let execs = build_executions(text, &natives, 0);

    // Exact XML duplicate of call_1 is skipped: native wins, no double exec.
    assert_eq!(execs.len(), 2);
    assert_eq!(execs[0].tool, "skill.search");
    assert_eq!(execs[0].tool_call_id.as_deref(), Some("call_1"));
    assert_eq!(execs[1].tool, "memory.search");
    // Stable ids follow act_base.
    assert_eq!(execs[0].id, "call_1");
    assert_eq!(execs[1].id, "call_2");
}

#[test]
fn empty_name_native_becomes_failed_result_not_drop() {
    let natives = vec![native("call_9", "", r#"{"query":"x"}"#)];
    let execs = build_executions("plain", &natives, 3);
    assert_eq!(execs.len(), 1);
    assert_eq!(execs[0].status, ToolStatus::Failed);
    let msg = execs[0].to_tool_result(4000);
    assert!(msg.contains("status=\"err\""));
    assert!(msg.contains("no name"));
}

#[test]
fn orphaned_action_block_triggers_nudge_instead_of_silent_finish() {
    assert!(has_orphaned_action_block(
        r#"doing it <action tool="terminal">{"command":"#
    ));
    assert!(!has_orphaned_action_block("plain answer, no action"));
    assert!(!has_orphaned_action_block(
        r#"<action tool="terminal">{"command":"ls"}</action>"#
    ));
}

#[test]
fn multiple_distinct_calls_keep_text_then_native_order() {
    // No duplicates here: XML grep + native memory.search stay in
    // native-first deterministic order.
    let text = r#"<action tool="grep">{"pattern":"a","path":"."}</action>"#;
    let natives = vec![native("call_1", "memory.search", r#"{"query":"b"}"#)];
    let execs = build_executions(text, &natives, 5);
    assert_eq!(execs.len(), 2);
    assert_eq!(execs[0].tool, "memory.search");
    assert_eq!(execs[1].tool, "grep");
    assert_eq!(execs[1].start.is_some(), true);
}

#[tokio::test]
async fn one_successful_tool_call_yields_ok_result() {
    let (gw, base) = test_gw("success");
    let out = argus_lib::tools::exec(
        &gw,
        "skill.search",
        r#"{"query":""}"#,
        "ask",
        false,
        true,
        None,
    )
    .await;
    assert!(out.is_ok(), "skill.search with empty query must succeed");
    let _ = std::fs::remove_dir_all(&base);
}

#[tokio::test]
async fn unknown_tool_becomes_structured_error() {
    let (gw, base) = test_gw("unknown");
    let err = argus_lib::tools::exec(&gw, "nope.tool", "{}", "ask", false, true, None)
        .await
        .expect_err("unknown tool must fail");
    assert!(err.contains("unknown tool"), "got: {err}");

    // And the execution wrapper still formats exactly one err result.
    let mut execs = build_executions(r#"<action tool="nope.tool">{}</action>"#, &[], 0);
    assert_eq!(execs.len(), 1);
    let e = &mut execs[0];
    e.begin();
    e.fail(err);
    assert_eq!(e.result_status(), "err");
    assert!(e.to_tool_result(4000).contains(r#"tool="nope.tool""#));
    let _ = std::fs::remove_dir_all(&base);
}

#[tokio::test]
async fn malformed_args_become_error_result() {
    let (gw, base) = test_gw("malformed");
    let err = argus_lib::tools::exec(&gw, "skill.read", "not-json", "ask", false, true, None)
        .await
        .expect_err("invalid JSON must fail");
    assert!(
        err.contains("not valid JSON"),
        "malformed args must surface as JSON error, got: {err}"
    );
    let _ = std::fs::remove_dir_all(&base);
}

#[tokio::test]
async fn tool_returning_error_surfaces_message() {
    let (gw, base) = test_gw("tool-err");
    // Valid JSON but missing required field -> tool-level error.
    let err = argus_lib::tools::exec(&gw, "skill.read", "{}", "ask", false, true, None)
        .await
        .expect_err("missing name must fail");
    assert!(err.contains("missing name"), "got: {err}");
    let _ = std::fs::remove_dir_all(&base);
}

#[tokio::test]
async fn two_step_results_append_in_order() {
    let (gw, base) = test_gw("twostep");
    // Round 1: two accepted calls, executed sequentially in order.
    let mut round1 = build_executions(
        r#"<action tool="skill.search">{"query":"a"}</action>"#,
        &[native("call_1", "memory.search", r#"{"query":"b"}"#)],
        0,
    );
    assert_eq!(round1.len(), 2);

    let mut results: Vec<String> = vec![];
    for e in round1.iter_mut() {
        e.begin();
        match argus_lib::tools::exec(&gw, &e.tool, &e.args, "ask", false, true, None).await {
            Ok(body) => e.succeed(body),
            Err(err) => e.fail(err),
        }
        assert!(e.status.is_terminal());
        results.push(e.to_tool_result(4000));
    }
    assert_eq!(results.len(), 2);
    assert!(results[0].contains(r#"tool="memory.search""#));
    assert!(results[1].contains(r#"tool="skill.search""#));

    // Round 2: next model step continues ids after round 1 (act_base + len).
    let round2 = build_executions(
        r#"<action tool="grep">{"pattern":"z","path":"."}</action>"#,
        &[],
        round1.len(),
    );
    assert_eq!(round2.len(), 1);
    assert_eq!(round2[0].id, "a2");
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn native_calls_carry_ids_for_live_synthesis() {
    let execs = build_executions("", &[native("c1", "doc.create", "{}")], 0);
    assert_eq!(execs.len(), 1);
    assert!(execs[0].tool_call_id.is_some());

    let execs = build_executions(
        r#"<action tool="terminal">{"command":"ls"}</action>"#,
        &[],
        0,
    );
    assert_eq!(execs.len(), 1);
    assert!(execs[0].tool_call_id.is_none());
}

#[tokio::test]
async fn code_run_is_gated_and_needs_a_command() {
    use argus_lib::tools::{exec, is_mutating};

    assert!(is_mutating("code.run"));

    let (gw, _base) = test_gw("code-run");
    let err = exec(&gw, "code.run", "{}", "never", false, true, None)
        .await
        .unwrap_err();
    assert_eq!(err, "missing command");
}
