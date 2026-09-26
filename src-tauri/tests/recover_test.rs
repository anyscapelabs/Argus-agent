//! Tests for the additive recovery classification layer.
//!
//! Classification is a pure function over error strings the codebase already
//! emits; retry budgets are enforced by `run_bounded` / `exec_with_recovery`.
//! Browser/computer/web/MCP implementations are untouched — only their
//! existing messages are classified.

use argus_lib::tools::recover::{classify, exec_with_recovery, run_bounded, RecoveryKind};
use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::Mutex;

fn test_gw(tag: &str) -> (argus_lib::gateway::Gateway, std::path::PathBuf) {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    conn.execute_batch(argus_lib::gateway::schema::MIGRATE)
        .unwrap();
    argus_lib::sessions::store::migrate(&conn).unwrap();
    argus_lib::skills::store::migrate(&conn).unwrap();
    argus_lib::library::store::migrate(&conn).unwrap();
    argus_lib::memory::store::migrate(&conn).unwrap();

    let base = std::env::temp_dir().join(format!("argus-recover-{}-{tag}", std::process::id()));
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
        jobs_dir: std::env::temp_dir().join("argus-jobs"),
        jobs: Mutex::new(HashMap::new()),
        events: Mutex::new(HashMap::new()),
        turns: Mutex::new(HashSet::new()),
        watching: Mutex::new(None),
    };

    (gw, base)
}

#[test]
fn retry_policy_is_bounded_per_category() {
    // No automatic blind retry.
    for kind in [
        RecoveryKind::InvalidArguments,
        RecoveryKind::ToolNotFound,
        RecoveryKind::PermissionDenied,
        RecoveryKind::ApprovalRequired,
        RecoveryKind::AuthenticationRequired,
        RecoveryKind::NotFound,
        RecoveryKind::StaleReference,
        RecoveryKind::Fatal,
    ] {
        assert_eq!(kind.max_attempts(), 1, "{}", kind.as_str());
        assert!(!kind.may_retry(), "{}", kind.as_str());
    }

    // At most one automatic retry, never indefinite.
    for kind in [
        RecoveryKind::Timeout,
        RecoveryKind::RateLimited,
        RecoveryKind::Recoverable,
    ] {
        assert_eq!(kind.max_attempts(), 2, "{}", kind.as_str());
        assert!(kind.may_retry(), "{}", kind.as_str());
    }
}

#[test]
fn fatal_error_has_no_retry() {
    assert_eq!(
        classify("unknown tool frobnicate"),
        RecoveryKind::ToolNotFound
    );
    assert_eq!(
        classify("same action 3 times without visible progress — change approach"),
        RecoveryKind::Fatal
    );
    assert_eq!(
        classify("some weird unexpected failure"),
        RecoveryKind::Fatal
    );
    assert_eq!(
        classify("no accessibility bus (is at-spi2 running?)"),
        RecoveryKind::Fatal
    );
}

#[test]
fn invalid_arguments_have_no_blind_retry() {
    for msg in [
        "action body is not valid JSON",
        "missing name",
        "missing content",
        "missing ref",
        "missing url",
        "terminal needs a command",
        "grep needs a pattern",
        "fs.write needs a path",
        "url must start with http:// or https://",
        "memory content must be 1-8192 bytes",
        "skill name must be kebab-case (lowercase, digits, dashes)",
        "link needs two distinct ids",
        "doc content is empty",
        "unsupported kind: use docx, pdf, pptx, xlsx, csv, md or txt",
        "bad extension",
        "skill 'x' already exists, update it instead",
        "key must be a combo like Return, ctrl+c, alt+Tab",
    ] {
        assert_eq!(classify(msg), RecoveryKind::InvalidArguments, "{msg}");
    }
    assert!(!RecoveryKind::InvalidArguments.may_retry());
}

#[test]
fn stale_references_classify_without_blind_retry() {
    assert_eq!(
        classify("stale ref — run browser.read for a fresh element list"),
        RecoveryKind::StaleReference
    );
    assert_eq!(
        classify("unknown ref — run browser.open or browser.read for a fresh list"),
        RecoveryKind::StaleReference
    );
    assert_eq!(
        classify("unknown ref — run computer.observe for a fresh list"),
        RecoveryKind::StaleReference
    );
    assert_eq!(
        classify("no screenshot yet — run computer.screen first"),
        RecoveryKind::StaleReference
    );
    // Re-observe recovery is future work: identical retry would fail
    // identically today, so only classification is enforced for now.
    assert!(!RecoveryKind::StaleReference.may_retry());
}

#[test]
fn permission_denied_has_no_retry() {
    for msg in [
        "action denied by user",
        "that field is a password field — the user types it themselves; tell them to enter it",
        "url looks like it carries a credential — remove the token from the url",
        "/root/x: Permission denied (os error 13)",
    ] {
        assert_eq!(classify(msg), RecoveryKind::PermissionDenied, "{msg}");
    }
    assert!(!RecoveryKind::PermissionDenied.may_retry());
}

#[test]
fn approval_and_auth_stop_for_user_action() {
    assert_eq!(
        classify("blocked: this session asks before acting; switch its permission to never to allow writes"),
        RecoveryKind::ApprovalRequired
    );
    assert!(!RecoveryKind::ApprovalRequired.may_retry());

    for msg in [
        "Argus extension not connected",
        "no API key stored for OpenAI — reconnect it in Providers settings",
        "OpenAI rejected this API key",
    ] {
        assert_eq!(classify(msg), RecoveryKind::AuthenticationRequired, "{msg}");
    }
    assert!(!RecoveryKind::AuthenticationRequired.may_retry());
}

#[test]
fn not_found_returns_to_model_without_blind_retry() {
    for msg in [
        "skill 'notes' not found",
        "memory not found",
        "item not found",
        "source file '/tmp/x' not found",
        "session abc not found",
        "app file vanished",
    ] {
        assert_eq!(classify(msg), RecoveryKind::NotFound, "{msg}");
    }
    assert!(!RecoveryKind::NotFound.may_retry());
}

#[test]
fn timeout_rate_limited_and_recoverable_allow_one_retry() {
    for msg in [
        "grep timed out after 30s",
        "extension timed out",
        "command timed out after 120s",
    ] {
        assert_eq!(classify(msg), RecoveryKind::Timeout, "{msg}");
    }
    for msg in [
        "search engine served a bot-check page — wait a moment and retry, or reword the query",
        "search returned no results (the engine may be rate-limiting automated queries — try again in a moment)",
        "page served a bot check (access denied) instead of content — try another source, not another query on the same engine",
        "provider keeps returning the same rate limit — likely out of quota; try another model",
    ] {
        assert_eq!(classify(msg), RecoveryKind::RateLimited, "{msg}");
    }
    for msg in [
        "navigation failed: net error",
        "chrome launch failed: no chrome",
        "search request failed: timed out",
        "browser session missing",
    ] {
        let kind = classify(msg);
        assert!(
            kind == RecoveryKind::Recoverable || kind == RecoveryKind::Timeout,
            "{msg} -> {}",
            kind.as_str()
        );
    }
    // "search request failed: timed out" contains "timed out": Timeout wins as
    // the more specific transient class; both allow exactly one retry.
    assert_eq!(RecoveryKind::Timeout.max_attempts(), 2);
    assert_eq!(RecoveryKind::RateLimited.max_attempts(), 2);
    assert_eq!(RecoveryKind::Recoverable.max_attempts(), 2);
}

#[tokio::test]
async fn timeout_retries_at_most_once_then_succeeds() {
    use std::sync::atomic::{AtomicU32, Ordering};
    let calls = AtomicU32::new(0);
    let (res, attempts) = run_bounded(|| async {
        let n = calls.fetch_add(1, Ordering::SeqCst) + 1;
        if n < 2 {
            Err("grep timed out after 30s".to_string())
        } else {
            Ok("hits".to_string())
        }
    })
    .await;
    assert!(res.is_ok());
    assert_eq!(attempts, 2);
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn rate_limited_retry_is_bounded() {
    use std::sync::atomic::{AtomicU32, Ordering};
    let calls = AtomicU32::new(0);
    let (res, attempts) = run_bounded(|| async {
        calls.fetch_add(1, Ordering::SeqCst);
        Err("search engine served a bot-check page — wait a moment and retry".to_string())
    })
    .await;
    assert!(res.is_err());
    assert_eq!(attempts, 2, "rate-limited must stop after one retry");
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn repeated_failure_stops_after_retry_budget() {
    // Retryable kind, always failing: exactly 2 total attempts.
    let (res, attempts) =
        run_bounded(|| async { Err("grep timed out after 30s".to_string()) }).await;
    assert!(res.is_err());
    assert_eq!(attempts, 2);

    // Fatal kind, always failing: a single attempt, no retry.
    let (res, attempts) = run_bounded(|| async { Err("unknown tool nope".to_string()) }).await;
    assert!(res.is_err());
    assert_eq!(attempts, 1);

    // Success first try: a single attempt.
    let (res, attempts) = run_bounded(|| async { Ok::<_, String>("ok".into()) }).await;
    assert!(res.is_ok());
    assert_eq!(attempts, 1);
}

#[tokio::test]
async fn real_tool_failures_do_not_retry() {
    let (gw, base) = test_gw("no-retry");
    let app = tauri::test::mock_app();
    let app = &app.handle().clone();

    // Fatal: unknown tool.
    let out = exec_with_recovery(app, &gw, "nope.tool", "{}", "never", false, true, None).await;
    assert!(out.result.is_err());
    assert_eq!(out.attempts, 1);
    assert_eq!(out.kind, Some(RecoveryKind::ToolNotFound));

    // Invalid arguments: malformed JSON.
    let out = exec_with_recovery(
        app,
        &gw,
        "skill.read",
        "not-json",
        "never",
        false,
        true,
        None,
    )
    .await;
    assert!(out.result.is_err());
    assert_eq!(out.attempts, 1);
    assert_eq!(out.kind, Some(RecoveryKind::InvalidArguments));

    // Invalid arguments: missing required field.
    let out = exec_with_recovery(app, &gw, "skill.read", "{}", "never", false, true, None).await;
    assert!(out.result.is_err());
    assert_eq!(out.attempts, 1);
    assert_eq!(out.kind, Some(RecoveryKind::InvalidArguments));

    let _ = std::fs::remove_dir_all(&base);
}

#[tokio::test]
async fn real_denied_write_does_not_retry_and_reports_once() {
    let (gw, base) = test_gw("denied");
    let target = base.join("proc-denied.txt");

    // Force an OS-level denial via the read-only /proc tree when available;
    // otherwise any unwritable path still proves attempts == 1.
    let proc_target = "/proc/argus-recover-must-not-exist-zzz/out.txt";
    let args = serde_json::json!({ "path": proc_target, "content": "x" }).to_string();
    let app = tauri::test::mock_app();
    let out = exec_with_recovery(
        &app.handle().clone(),
        &gw,
        "fs.write",
        &args,
        "never",
        false,
        true,
        None,
    )
    .await;
    assert!(out.result.is_err());
    assert_eq!(
        out.attempts, 1,
        "denied/failed write must never blind-retry"
    );

    // The one structured error still becomes the single model-facing result.
    let mut exec = argus_lib::tools::build_executions(
        &format!(r#"<action tool="fs.write">{args}</action>"#),
        &[],
        0,
    )
    .pop()
    .unwrap();
    exec.begin();
    exec.fail(out.result.err().unwrap());
    assert!(exec.status.is_terminal());
    let msg = exec.to_tool_result(4000);
    assert!(msg.contains(r#"status="err""#));
    assert!(!target.exists());

    let _ = std::fs::remove_dir_all(&base);
}

#[tokio::test]
async fn real_success_uses_single_attempt() {
    let (gw, base) = test_gw("success");
    let app = tauri::test::mock_app();
    let app = &app.handle().clone();
    let out = exec_with_recovery(
        app,
        &gw,
        "skill.search",
        r#"{"query":""}"#,
        "never",
        false,
        true,
        None,
    )
    .await;
    assert!(out.result.is_ok());
    assert_eq!(out.attempts, 1);
    assert_eq!(out.kind, None);
    let _ = std::fs::remove_dir_all(&base);
}
