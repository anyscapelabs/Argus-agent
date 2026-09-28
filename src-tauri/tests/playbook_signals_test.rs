// Signals must come from things Argus observed, never from prose a model
// wrote. Each test here proves the negative case: model text that looks like
// a harness fact must not become a lesson.

use argus_lib::playbook::store::{self, Kind, Scope};
use argus_lib::tools::{build_executions, ToolStatus};

fn db() -> rusqlite::Connection {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    conn.execute_batch(argus_lib::gateway::schema::MIGRATE)
        .unwrap();
    store::migrate(&conn).unwrap();
    conn
}

fn host() -> String {
    argus_lib::sessions::ext_install::host_id()
}

fn exec_with(
    tool: &str,
    args: &str,
    err: Option<&str>,
    out: &str,
) -> argus_lib::tools::ToolExecution {
    let mut e = build_executions(&format!("<action tool=\"{tool}\">{args}</action>"), &[], 0)
        .pop()
        .expect("one execution");
    e.begin();

    if let Some(m) = err {
        e.fail(m.into());
    } else {
        e.succeed(out.into());
    }

    e
}

// The empty-args trip is a model fact and must land on the model.
#[test]
fn an_empty_args_failure_is_a_model_signal() {
    let conn = db();
    let e = exec_with("terminal", "{}", Some("call for `terminal` arrived with empty arguments — nothing ran; send it again with its arguments"), "");

    argus_lib::sessions::blocks::observe_exec(&conn, "prov/m", &e, false);

    let got = store::signals_for(&conn, Kind::EmptyArgs, "prov/m").unwrap();
    assert_eq!(got.len(), 1, "one signal on the model");
    assert_eq!(got[0].seen, 1);
    assert!(store::signals_for(&conn, Kind::EmptyArgs, &host())
        .unwrap()
        .is_empty());
}

// The missing-cwd refusal comes back as an error string, not a note.
#[test]
fn a_missing_cwd_refusal_is_a_host_signal() {
    let conn = db();
    let e = exec_with(
        "terminal",
        r#"{"command":"ls","profile":"project"}"#,
        Some("project profile needs cwd: without it the command would be confined to the launch directory (/x); pass cwd=<project dir> or use profile=host"),
        "",
    );

    argus_lib::sessions::blocks::observe_exec(&conn, "prov/m", &e, false);

    assert_eq!(
        store::signals_for(&conn, Kind::SandboxNoCwd, &host())
            .unwrap()
            .len(),
        1
    );
}

// The denial note the sandbox layer really appends, at the tail.
fn denial_tail() -> String {
    format!(
        "exit 1\n{}: this profile confines the command to /root. \
         Rescope with cwd=<dir inside it>, or rerun with profile=host.",
        argus_lib::tools::sandbox::DENIAL_NOTE
    )
}

// A sandbox refusal is this machine's doing. Filing it against the model
// would blame it for a sandbox it never controlled.
#[test]
fn a_sandbox_denial_is_a_host_signal() {
    let conn = db();
    let e = exec_with("terminal", r#"{"command":"ls"}"#, None, &denial_tail());

    argus_lib::sessions::blocks::observe_exec(&conn, "prov/m", &e, false);

    assert_eq!(
        store::signals_for(&conn, Kind::SandboxDenied, &host())
            .unwrap()
            .len(),
        1
    );
    assert!(
        store::signals_for(&conn, Kind::SandboxDenied, "prov/m")
            .unwrap()
            .is_empty(),
        "must not be filed against the model"
    );
}

// A file the agent happened to read containing the marker is not a refusal.
// The note is checked at the tail, because that is where Argus appends it.
#[test]
fn the_marker_earlier_in_the_output_is_not_a_denial() {
    let conn = db();
    let note = argus_lib::tools::sandbox::DENIAL_NOTE;
    let body = format!(
        "exit 0\n{note}\nfound in README\nindex.js\npackage.json\n{}\nmore output",
        "x".repeat(400)
    );
    let e = exec_with("terminal", r#"{"command":"cat README"}"#, None, &body);

    argus_lib::sessions::blocks::observe_exec(&conn, "prov/m", &e, false);

    assert!(
        store::signals_for(&conn, Kind::SandboxDenied, &host())
            .unwrap()
            .is_empty(),
        "a file containing the marker is not a denial"
    );
}

#[test]
fn thrashing_is_a_model_signal() {
    let conn = db();
    let e = exec_with(
        "terminal",
        r#"{"command":"ls"}"#,
        Some("retried the same approach without progress — change approach or ask the user"),
        "",
    );

    argus_lib::sessions::blocks::observe_exec(&conn, "prov/m", &e, true);

    assert_eq!(
        store::signals_for(&conn, Kind::Thrashing, "prov/m")
            .unwrap()
            .len(),
        1
    );
}

// The scopes must stay disjoint even when a turn produces both kinds.
#[test]
fn one_turn_can_produce_both_scopes() {
    let conn = db();

    let empty = exec_with("terminal", "{}", Some("call for `x` arrived with empty arguments — nothing ran; send it again with its arguments"), "");
    argus_lib::sessions::blocks::observe_exec(&conn, "prov/m", &empty, false);

    let denied = exec_with("terminal", r#"{"command":"ls"}"#, None, &denial_tail());
    argus_lib::sessions::blocks::observe_exec(&conn, "prov/m", &denied, false);

    assert_eq!(
        store::signals_for(&conn, Kind::EmptyArgs, "prov/m")
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        store::signals_for(&conn, Kind::SandboxDenied, &host())
            .unwrap()
            .len(),
        1
    );
}

// A successful call is not a signal of anything.
#[test]
fn a_successful_call_records_nothing() {
    let conn = db();
    let e = exec_with("terminal", r#"{"command":"ls"}"#, None, "exit 0\nfile");

    argus_lib::sessions::blocks::observe_exec(&conn, "prov/m", &e, false);

    for kind in Kind::all() {
        assert!(
            store::signals_for(&conn, kind, "prov/m")
                .unwrap()
                .is_empty(),
            "{kind:?}"
        );
    }
}

// An exec that never ran (pre-failed) has no observation to offer.
#[test]
fn a_pre_failed_exec_records_nothing() {
    let conn = db();
    let e = build_executions(r#"<action tool="terminal">{}</action>"#, &[], 0)
        .pop()
        .unwrap();
    assert_eq!(e.status, ToolStatus::Failed);

    // A failed-but-never-executed exec carries an error string, so the caller
    // is the only thing that can tell them apart. This asserts the shape the
    // caller relies on.
    assert!(e.error.is_some());
    assert!(e.result.is_none());
}
