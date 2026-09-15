//! Hermetic tests for computer-use grounding.
//!
//! The container runs on the developer's live X11 desktop, so no test here
//! may observe the real screen, synthesize input, or launch apps. Everything
//! below runs without desktop contact: pure helpers plus `tools::exec`
//! failure paths that return before any X11/AT-SPI call.
//!
//! Deliberately not covered here (needs a controlled X11 + AT-SPI fixture
//! with a test app — unavailable; Xvfb is not installed and the live desktop
//! must not be disturbed): observe output content, successful act/click/type
//! against real elements, live window-id existence checks, and end-to-end
//! verification on real state changes.

use argus_lib::tools::computer::x11::{valid_window_id, verify_windows_note, WinState};
use argus_lib::tools::recover::{classify, RecoveryKind};
use std::collections::HashMap;
use std::sync::Mutex;

fn test_gw() -> (argus_lib::gateway::Gateway, std::path::PathBuf) {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    conn.execute_batch(argus_lib::gateway::schema::MIGRATE)
        .unwrap();
    argus_lib::sessions::store::migrate(&conn).unwrap();
    argus_lib::skills::store::migrate(&conn).unwrap();
    argus_lib::library::store::migrate(&conn).unwrap();
    argus_lib::memory::store::migrate(&conn).unwrap();

    let base = std::env::temp_dir().join(format!("argus-cground-{}", std::process::id()));
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

async fn exec(gw: &argus_lib::gateway::Gateway, tool: &str, args: &str) -> Result<String, String> {
    argus_lib::tools::exec(gw, tool, args, "never", false, true, None).await
}

fn state(ids: &[&str], active: Option<&str>) -> WinState {
    WinState {
        ids: ids.iter().map(|s| s.to_string()).collect(),
        active: active.map(str::to_string),
    }
}

#[test]
fn window_ids_validate_without_desktop() {
    assert!(valid_window_id("0x03c00007"));
    assert!(valid_window_id("0xabcdef"));
    assert!(!valid_window_id(""));
    assert!(!valid_window_id("zz-top"));
    assert!(!valid_window_id("0x03c 0007"));
}

#[test]
fn verification_notes_need_real_change() {
    let before = state(&["0x1", "0x2"], Some("0x1"));

    let after = state(&["0x1", "0x2", "0x3"], Some("0x1"));
    assert_eq!(
        verify_windows_note(&before, &after),
        Some("note: verified: window list changed".into())
    );

    let after = state(&["0x2", "0x1"], Some("0x1"));
    assert_eq!(verify_windows_note(&before, &after), None);

    let after = state(&["0x1", "0x2"], Some("0x2"));
    assert_eq!(
        verify_windows_note(&before, &after),
        Some("note: verified: active window changed".into())
    );

    assert_eq!(verify_windows_note(&before, &before.clone()), None);

    let unknown = state(&[], None);
    assert_eq!(verify_windows_note(&unknown, &unknown.clone()), None);
    assert_eq!(
        verify_windows_note(&before, &unknown),
        Some("note: verified: window list changed".into())
    );
}

#[test]
fn grounding_failures_classify_correctly() {
    assert_eq!(
        classify("stale ref 5 from observation 2 — observation 9 is current; run computer.observe for a fresh list"),
        RecoveryKind::StaleReference
    );
    assert_eq!(
        classify("stale screenshot from observation 2 — observation 9 is current; run computer.screen for a fresh screenshot"),
        RecoveryKind::StaleReference
    );
    assert_eq!(
        classify("focus changed since the screenshot (11 → 22) — run computer.screen for a fresh screenshot"),
        RecoveryKind::StaleReference
    );
    assert_eq!(
        classify("unknown ref — run computer.observe for a fresh list"),
        RecoveryKind::StaleReference
    );
    assert_eq!(
        classify("window id 0xdeadbeef not found — run computer.observe for a fresh list"),
        RecoveryKind::NotFound
    );
}

#[tokio::test]
async fn stale_element_ref_is_rejected_without_desktop() {
    let (gw, base) = test_gw();

    let err = exec(&gw, "computer.act", r#"{"ref":5,"observation":99}"#)
        .await
        .expect_err("superseded observation must be stale");
    assert!(
        err.contains("stale ref 5 from observation 99"),
        "got: {err}"
    );
    assert_eq!(classify(&err), RecoveryKind::StaleReference);

    let err = exec(&gw, "computer.act", r#"{"ref":99}"#)
        .await
        .expect_err("out of range ref");
    assert!(err.contains("unknown ref"), "got: {err}");

    let _ = std::fs::remove_dir_all(&base);
}

#[tokio::test]
async fn stale_screenshot_coords_are_rejected_without_desktop() {
    let (gw, base) = test_gw();

    let err = exec(&gw, "computer.click", r#"{"x":10,"y":10,"observation":99}"#)
        .await
        .expect_err("superseded screenshot must be stale");
    assert!(
        err.contains("stale screenshot from observation 99"),
        "got: {err}"
    );
    assert_eq!(classify(&err), RecoveryKind::StaleReference);

    let err = exec(&gw, "computer.scroll", r#"{"x":10,"y":10,"observation":7}"#)
        .await
        .expect_err("superseded screenshot must be stale");
    assert!(err.contains("stale screenshot"), "got: {err}");

    let _ = std::fs::remove_dir_all(&base);
}

#[tokio::test]
async fn unknown_window_id_is_structured_failure() {
    let (gw, base) = test_gw();

    let err = exec(
        &gw,
        "computer.window",
        r#"{"action":"activate","id":"zz-top"}"#,
    )
    .await
    .expect_err("bad hex id");
    assert!(err.contains("hex window id"), "got: {err}");

    let err = exec(&gw, "computer.window", r#"{"action":"activate"}"#)
        .await
        .expect_err("missing id");
    assert!(err.contains("missing id"), "got: {err}");

    let _ = std::fs::remove_dir_all(&base);
}

#[tokio::test]
async fn computer_failure_is_one_structured_result() {
    let (gw, base) = test_gw();

    let failed = exec(&gw, "computer.click", r#"{"y":10}"#)
        .await
        .expect_err("missing x must fail");
    assert!(
        failed.contains("no screenshot yet") || failed.contains("missing x"),
        "got: {failed}"
    );

    let mut exec_state = argus_lib::tools::build_executions(
        r#"<action tool="computer.click">{"y":10}</action>"#,
        &[],
        0,
    )
    .pop()
    .unwrap();
    exec_state.begin();
    exec_state.fail(failed);
    assert!(exec_state.status.is_terminal());
    let results = vec![exec_state.to_tool_result(4000)];
    assert_eq!(results.len(), 1);
    assert!(results[0].contains(r#"tool="computer.click" status="err""#));

    let _ = std::fs::remove_dir_all(&base);
}

#[tokio::test]
async fn computer_permission_gate_stays_closed() {
    let (gw, base) = test_gw();

    let err = argus_lib::tools::exec(
        &gw,
        "computer.click",
        r#"{"x":10,"y":10}"#,
        "ask",
        false,
        false,
        None,
    )
    .await
    .expect_err("mutating without approval stays blocked");
    assert!(err.contains("asks before acting"), "got: {err}");
    assert_eq!(classify(&err), RecoveryKind::ApprovalRequired);

    let _ = std::fs::remove_dir_all(&base);
}
