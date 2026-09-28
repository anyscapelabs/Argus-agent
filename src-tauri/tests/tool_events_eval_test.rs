use argus_lib::gateway::schema::ToolCall;
use argus_lib::sessions::events::from_execution;
use argus_lib::sessions::store;
use argus_lib::tools::{
    build_executions_styled, normalize_actions, render_actions, strip_actions, ToolCallStyle,
};

fn native(id: &str, name: &str, args: &str) -> ToolCall {
    ToolCall {
        id: id.into(),
        name: name.into(),
        args: args.into(),
    }
}

fn db() -> rusqlite::Connection {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    store::migrate(&conn).unwrap();
    conn.execute("INSERT INTO sessions (id, title) VALUES ('s1', 't')", [])
        .unwrap();
    conn
}

fn protocol_debris(s: &str) -> bool {
    s.contains("<action")
        || s.contains("<terminal")
        || s.contains("<tool_call")
        || s.contains("arg_key")
        || s.contains("arg_value")
}

// The class-level claim: a native turn persists prose with no record markup,
// while the event rows carry everything the cards, the history, and the audit
// need. If this holds, no leak shape can recur through fresh turns.
#[test]
fn native_turn_persists_prose_only_with_complete_events() {
    let conn = db();
    let natives = vec![native("c1", "terminal", r#"{"command":"ls -la"}"#)];
    let model_text = "Checking the directory.";

    let base = normalize_actions(model_text);
    let execs = build_executions_styled(&base, &natives, 0, ToolCallStyle::Native);
    assert_eq!(execs.len(), 1);

    // What chat.rs persists for native turns.
    let persisted = strip_actions(&base);
    assert!(!protocol_debris(&persisted), "{persisted}");
    assert!(persisted.contains("Checking the directory."), "{persisted}");

    // What lands in tool_events for the same execution.
    let mut e = execs.into_iter().next().unwrap();
    e.begin();
    e.succeed("exit 0\nfile".into());
    let got = store::add_event(&conn, &from_execution(&e, "m1", "s1")).unwrap();
    assert_eq!(got.tool, "terminal");
    assert_eq!(got.args_json, r#"{"command":"ls -la"}"#);
    assert_eq!(got.status, "succeeded");
    assert_eq!(got.label, "$ ls -la", "{}", got.label);
    assert!(got.output.contains("file"), "{}", got.output);

    // History for this turn needs no markup at all: prose plus the native
    // tool_calls JSON the messages table already carries.
    let hist = strip_actions(&persisted);
    assert!(!protocol_debris(&hist), "{hist}");
}

// The duplicate that used to compound: native twin plus text twin. One
// execution, one event, prose-only text.
#[test]
fn duplicate_turn_runs_once_and_persists_once() {
    let conn = db();
    let natives = vec![native("c1", "terminal", r#"{"command":"ls"}"#)];
    let model_text = "ran it <action tool=\"terminal\">{\"command\":\"ls\"}</action> done";

    let base = normalize_actions(model_text);
    let execs = build_executions_styled(&base, &natives, 0, ToolCallStyle::Native);
    assert_eq!(execs.len(), 1, "the text twin must never execute");

    let persisted = strip_actions(&base);
    assert!(!protocol_debris(&persisted), "{persisted}");

    let mut e = execs.into_iter().next().unwrap();
    e.begin();
    e.succeed("exit 0\nx".into());
    store::add_event(&conn, &from_execution(&e, "m1", "s1")).unwrap();
    assert_eq!(store::list_events(&conn, "s1").unwrap().len(), 1);
}

// Degraded turns keep the old shape on purpose: text blocks execute and
// persist, events mirror them. Both readers agree.
#[test]
fn degraded_turn_keeps_text_blocks_and_mirrors_events() {
    let conn = db();
    let model_text = "<action tool=\"terminal\">{\"command\":\"ls\"}</action>";

    let base = normalize_actions(model_text);
    let execs = build_executions_styled(&base, &[], 0, ToolCallStyle::GlmXml);
    assert_eq!(execs.len(), 1, "degraded text must still execute");

    let persisted = render_actions(&base, &execs);
    assert!(
        persisted.contains("<action tool=\"terminal\""),
        "{persisted}"
    );

    let mut e = execs.into_iter().next().unwrap();
    e.begin();
    e.succeed("exit 0\nx".into());
    let got = store::add_event(&conn, &from_execution(&e, "m1", "s1")).unwrap();
    assert_eq!(got.label, "$ ls", "{}", got.label);
}

// Legacy rows have no events: the text parser remains their only reader.
#[test]
fn legacy_rows_have_no_events_to_prefer() {
    let conn = db();
    assert!(store::list_events(&conn, "s1").unwrap().is_empty());
}

// Multi-line commands never touch markup: the command is a column value.
#[test]
fn multiline_command_survives_as_data() {
    let conn = db();
    let natives = vec![native(
        "c1",
        "terminal",
        "{\"command\":\"for f in *; do\\necho $f\\ndone\"}",
    )];

    let execs = build_executions_styled("looping", &natives, 0, ToolCallStyle::Native);
    let mut e = execs.into_iter().next().unwrap();
    e.begin();
    e.succeed("exit 0\na".into());
    let got = store::add_event(&conn, &from_execution(&e, "m1", "s1")).unwrap();

    assert!(
        got.args_json.contains("\\n"),
        "newline kept as data (JSON-escaped): {}",
        got.args_json
    );
    let v: serde_json::Value = serde_json::from_str(&got.args_json).unwrap();
    assert!(
        v["command"].as_str().unwrap_or("").contains('\n'),
        "round-trips to a real newline"
    );
    assert_eq!(
        got.label, "$ for f in *; do\necho $f\ndone",
        "{}",
        got.label
    );
}
