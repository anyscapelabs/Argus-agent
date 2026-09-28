use argus_lib::gateway::schema::ToolCall;
use argus_lib::prompt::config::{tool_style, upgrade_tool_style};
use argus_lib::tools::{
    build_executions_styled, has_native_text_duplicate, render_actions, ToolCallStyle,
};

fn native(id: &str, name: &str, args: &str) -> ToolCall {
    ToolCall {
        id: id.into(),
        name: name.into(),
        args: args.into(),
    }
}

fn memdb() -> rusqlite::Connection {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    conn.execute_batch(argus_lib::gateway::schema::MIGRATE)
        .unwrap();
    conn
}

fn seed_model(conn: &rusqlite::Connection, id: &str, caps: Option<&str>) {
    conn.execute(
        "INSERT INTO models (id, display_name, family, capabilities) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![id, id, "", caps],
    )
    .unwrap();
}

// The seed: family names containing glm speak the XML template.
#[test]
fn glm_family_seeds_xml_style() {
    assert_eq!(
        ToolCallStyle::style_for_family(Some("zai-org/GLM-5.3-Fast")),
        ToolCallStyle::GlmXml
    );
    assert_eq!(
        ToolCallStyle::style_for_family(Some("glm-4.7")),
        ToolCallStyle::GlmXml
    );
    assert_eq!(
        ToolCallStyle::style_for_family(Some("anthropic/claude-sonnet")),
        ToolCallStyle::Native
    );
    assert_eq!(ToolCallStyle::style_for_family(None), ToolCallStyle::Native);
}

// Unknown, absent, or broken capabilities all mean Native. An unknown model
// is handled by the duplication upgrade, not by guessing here.
#[test]
fn caps_default_to_native_unless_xml() {
    assert_eq!(
        ToolCallStyle::from_caps(Some(r#"{"tool_call_style":"glm-xml"}"#)),
        ToolCallStyle::GlmXml
    );
    assert_eq!(ToolCallStyle::from_caps(None), ToolCallStyle::Native);
    assert_eq!(
        ToolCallStyle::from_caps(Some(r#"{"tools":true}"#)),
        ToolCallStyle::Native
    );
    assert_eq!(
        ToolCallStyle::from_caps(Some("not-json")),
        ToolCallStyle::Native
    );
}

// Merging the flag never drops the keys the catalog wrote.
#[test]
fn caps_merge_preserves_existing_keys() {
    let out = ToolCallStyle::caps_with_style(
        Some(r#"{"tools":true,"context":128000}"#),
        ToolCallStyle::GlmXml,
    );
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();

    assert_eq!(v["tool_call_style"], "glm-xml");
    assert_eq!(v["tools"], true);
    assert_eq!(v["context"], 128000);

    let fresh = ToolCallStyle::caps_with_style(None, ToolCallStyle::Native);
    let v: serde_json::Value = serde_json::from_str(&fresh).unwrap();
    assert_eq!(v["tool_call_style"], "native");
}

// The DB read: seeded flag wins, missing row or bad JSON fall back to Native.
#[test]
fn db_style_reads_seeded_flag() {
    let conn = memdb();
    seed_model(
        &conn,
        "baseten/zai-org/GLM-5.3-Fast",
        Some(r#"{"tools":true,"tool_call_style":"glm-xml"}"#),
    );
    seed_model(&conn, "prov/plain-model", Some(r#"{"tools":true}"#));
    seed_model(&conn, "prov/broken-model", Some("nope"));

    assert_eq!(
        tool_style(&conn, Some("baseten/zai-org/GLM-5.3-Fast")),
        ToolCallStyle::GlmXml
    );
    assert_eq!(
        tool_style(&conn, Some("prov/plain-model")),
        ToolCallStyle::Native
    );
    assert_eq!(
        tool_style(&conn, Some("prov/broken-model")),
        ToolCallStyle::Native
    );
    assert_eq!(
        tool_style(&conn, Some("prov/never-seen")),
        ToolCallStyle::Native
    );
    assert_eq!(tool_style(&conn, None), ToolCallStyle::Native);
}

// The runtime upgrade: one duplicate flips the flag, keeps other keys, and
// a missing row is a silent no-op (never fails a turn).
#[test]
fn duplicate_upgrades_style_in_db() {
    let conn = memdb();
    seed_model(
        &conn,
        "prov/new-model",
        Some(r#"{"tools":true,"context":64000}"#),
    );
    assert_eq!(
        tool_style(&conn, Some("prov/new-model")),
        ToolCallStyle::Native
    );

    upgrade_tool_style(&conn, "prov/new-model");
    assert_eq!(
        tool_style(&conn, Some("prov/new-model")),
        ToolCallStyle::GlmXml
    );

    let caps: String = conn
        .query_row(
            "SELECT capabilities FROM models WHERE id = 'prov/new-model'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let v: serde_json::Value = serde_json::from_str(&caps).unwrap();
    assert_eq!(v["context"], 64000, "catalog keys must survive the merge");

    upgrade_tool_style(&conn, "prov/never-seen");
    assert_eq!(
        tool_style(&conn, Some("prov/never-seen")),
        ToolCallStyle::Native
    );
}

// The upgrade signature: native twin plus identical text twin.
#[test]
fn duplicate_across_channels_detected() {
    let natives = vec![native("c1", "terminal", r#"{"command":"ls"}"#)];
    let text = r#"ran it <action tool="terminal">{"command":"ls"}</action> done"#;

    assert!(has_native_text_duplicate(text, &natives));
}

#[test]
fn no_upgrade_without_both_channels() {
    let natives = vec![native("c1", "terminal", r#"{"command":"ls"}"#)];

    assert!(!has_native_text_duplicate("plain prose", &natives));
    assert!(!has_native_text_duplicate(
        r#"<action tool="terminal">{"command":"ls"}</action>"#,
        &[]
    ));

    // Same tool, different args: two real calls, not a duplicate.
    let other = r#"<action tool="terminal">{"command":"pwd"}</action>"#;
    assert!(!has_native_text_duplicate(other, &natives));
}

// Eval matrix: every row is a (style, channels) case with the exact expected
// outcome. Validity = args parse; selection = right tool runs; termination =
// no duplicate execution, ever.
#[test]
fn native_style_runs_api_only() {
    let natives = vec![native("c1", "terminal", r#"{"command":"ls"}"#)];
    let text = r#"ran it <action tool="terminal">{"command":"ls"}</action> done"#;

    let execs = build_executions_styled(text, &natives, 0, ToolCallStyle::Native);

    assert_eq!(execs.len(), 1, "the text twin must never execute");
    assert_eq!(execs[0].tool_call_id.as_deref(), Some("c1"));

    let persisted = render_actions(text, &execs);
    assert_eq!(
        persisted, r#"ran it <action tool="terminal">{"command":"ls"}</action> done"#,
        "one canonical record in place of the twin"
    );
}

#[test]
fn native_style_text_without_api_channel_runs_nothing() {
    let execs = build_executions_styled(
        r#"<action tool="terminal">{"command":"ls"}</action>"#,
        &[],
        0,
        ToolCallStyle::Native,
    );

    assert!(execs.is_empty(), "prose is never obeyed for native models");
}

#[test]
fn template_style_decodes_and_dedups() {
    let natives = vec![native("c1", "terminal", r#"{"command":"ls"}"#)];
    let text = r#"ran it <action tool="terminal">{"command":"ls"}</action> done"#;

    let execs = build_executions_styled(text, &natives, 0, ToolCallStyle::GlmXml);
    assert_eq!(execs.len(), 1, "duplicate runs once");

    let solo = build_executions_styled(
        r#"<action tool="terminal">{"command":"ls"}</action>"#,
        &[],
        0,
        ToolCallStyle::GlmXml,
    );
    assert_eq!(solo.len(), 1, "text-only template call still runs");
    assert_eq!(solo[0].tool, "terminal");
}
