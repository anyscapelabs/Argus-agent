use argus_lib::sessions::events::{from_execution, kind_of};
use argus_lib::sessions::store;
use argus_lib::tools::{build_executions, ToolStatus};

fn db() -> rusqlite::Connection {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    store::migrate(&conn).unwrap();
    conn.execute("INSERT INTO sessions (id, title) VALUES ('s1', 't')", [])
        .unwrap();
    conn
}

fn exec_for(text: &str) -> argus_lib::tools::ToolExecution {
    let mut e = build_executions(text, &[], 0).pop().expect("one execution");
    e.begin();
    e.succeed("exit 0\nhello".into());
    e
}

// What ran is what is stored: every field survives the round trip.
#[test]
fn event_round_trip_keeps_every_field() {
    let conn = db();
    let e = exec_for(r#"<action tool="terminal">{"command":"ls"}</action>"#);
    let n = from_execution(&e, "m1", "s1");

    let got = store::add_event(&conn, &n).unwrap();

    assert_eq!(got.tool, "terminal");
    assert_eq!(got.kind, "terminal");
    assert_eq!(got.args_json, r#"{"command":"ls"}"#);
    assert_eq!(got.status, "succeeded");
    assert!(got.output.contains("hello"), "{}", got.output);
    assert_eq!(got.message_id, "m1");
    assert_eq!(got.label, "$ ls", "got: {}", got.label);

    let all = store::list_events(&conn, "s1").unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].id, got.id);
}

// Superseding a message retries the turn; it does not un-run tools. Events
// are keyed by session, so the audit survives the retry.
#[test]
fn events_survive_message_supersede() {
    let conn = db();
    let e = exec_for(r#"<action tool="grep">{"pattern":"x"}</action>"#);
    store::add_event(&conn, &from_execution(&e, "m-old", "s1")).unwrap();

    conn.execute("INSERT INTO sessions (id, title) VALUES ('s2', 't')", [])
        .unwrap();
    assert!(store::list_events(&conn, "s1").unwrap().len() == 1);
    assert!(store::list_events(&conn, "s2").unwrap().is_empty());
}

// Kinds follow the tool, not the markup that used to carry them.
#[test]
fn kind_follows_tool_family() {
    assert_eq!(kind_of("terminal"), "terminal");
    assert_eq!(kind_of("bash.run"), "terminal");
    assert_eq!(kind_of("browser.open"), "browser");
    assert_eq!(kind_of("doc.create"), "document");
    assert_eq!(kind_of("code.run"), "sandbox");
    assert_eq!(kind_of("grep"), "action");
    assert_eq!(kind_of("skill.search"), "action");
}

// A database written before tool_events existed gains the table on migrate,
// with old rows untouched.
#[test]
fn old_database_gains_tool_events_on_migrate() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE sessions (id TEXT PRIMARY KEY, title TEXT NOT NULL);
         CREATE TABLE messages (id TEXT PRIMARY KEY, session_id TEXT NOT NULL, seq INTEGER NOT NULL,
           role TEXT NOT NULL, content TEXT NOT NULL);",
    )
    .unwrap();

    store::migrate(&conn).unwrap();

    let n: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='tool_events'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(n, 1);

    conn.execute("INSERT INTO sessions (id, title) VALUES ('s1', 't')", [])
        .unwrap();
    let e = exec_for(r#"<action tool="terminal">{"command":"ls"}</action>"#);
    store::add_event(&conn, &from_execution(&e, "m1", "s1")).unwrap();
    assert_eq!(store::list_events(&conn, "s1").unwrap().len(), 1);
}

// Display text is computed once at write time, not derived per reader.
#[test]
fn labels_mirror_card_titles() {
    let conn = db();

    let term = exec_for(r#"<action tool="terminal">{"command":"ls","label":"List"}</action>"#);
    let got = store::add_event(&conn, &from_execution(&term, "m1", "s1")).unwrap();
    assert_eq!(got.label, "List", "{}", got.label);
    assert_eq!(got.detail, "$ ls", "{}", got.detail);

    let mut b = build_executions(
        r#"<action tool="browser.open">{"url":"https://x.y"}</action>"#,
        &[],
        0,
    )
    .pop()
    .unwrap();
    b.begin();
    b.succeed("ok".into());
    let got = store::add_event(&conn, &from_execution(&b, "m2", "s1")).unwrap();
    assert_eq!(got.kind, "browser");
    assert_eq!(got.label, "Opened https://x.y", "{}", got.label);

    let mut d = build_executions(r#"<action tool="doc.create">{"name":"N"}</action>"#, &[], 0)
        .pop()
        .unwrap();
    d.begin();
    d.succeed("id=1\npath=p\nname=N\nkind=doc\next=md\npages=2".into());
    let got = store::add_event(&conn, &from_execution(&d, "m3", "s1")).unwrap();
    assert_eq!(got.label, "Created document N", "{}", got.label);
}

// Failed executions record the error, not an empty body.
#[test]
fn failed_status_and_error_are_kept() {
    let conn = db();
    let e = build_executions(r#"<action tool="terminal">{}</action>"#, &[], 0)
        .pop()
        .unwrap();
    assert_eq!(e.status, ToolStatus::Failed);

    let got = store::add_event(&conn, &from_execution(&e, "m1", "s1")).unwrap();
    assert_eq!(got.status, "failed");
    assert!(got.output.contains("empty arguments"), "{}", got.output);
}
