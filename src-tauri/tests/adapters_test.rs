use argus_lib::gateway::adapters::anthropic_content;
use argus_lib::gateway::adapters::openai_msgs;
use argus_lib::gateway::schema::{ToolCall, WireMsg};

fn msg(content: &str, images: Vec<String>) -> WireMsg {
    WireMsg {
        role: "user".into(),
        content: content.into(),
        images,
        ..Default::default()
    }
}

#[test]
fn openai_msgs_attaches_only_real_screenshots() {
    let dir = std::env::temp_dir().join(format!("argus-shot-test-{}", std::process::id()));
    let sub = dir.join("screenshots");
    std::fs::create_dir_all(&sub).unwrap();
    let real = sub.join("shot-t.png");
    std::fs::write(&real, b"pngbytes").unwrap();

    let msgs = vec![
        msg("plain", vec![]),
        msg("with shot", vec![real.to_string_lossy().into_owned()]),
        msg("fake", vec!["/tmp/screenshots/shot-missing.png".into()]),
    ];

    let out = openai_msgs(&msgs);

    assert_eq!(out[0]["content"], "plain");
    assert!(out[1]["content"].is_array());
    assert_eq!(out[1]["content"][0]["type"], "text");
    assert_eq!(out[1]["content"][1]["type"], "image_url");
    let url = out[1]["content"][1]["image_url"]["url"].as_str().unwrap();
    assert!(url.starts_with("data:image/png;base64,"));

    // missing file still serializes, just without an image part
    assert_eq!(out[2]["content"][0]["type"], "text");
    assert_eq!(out[2]["content"].as_array().unwrap().len(), 1);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn anthropic_content_switches_to_parts_with_images() {
    let plain = msg("hello", vec![]);
    assert_eq!(anthropic_content(&plain), serde_json::json!("hello"));

    let m = msg("shot", vec!["/tmp/screenshots/shot-x.png".into()]);
    let v = anthropic_content(&m);
    assert!(v.is_array());
    assert_eq!(v[0]["type"], "text");
    assert_eq!(v.as_array().unwrap().len(), 1);
}

#[test]
fn openai_msgs_round_trips_native_tool_calls() {
    let msgs = vec![
        WireMsg {
            role: "assistant".into(),
            content: "".into(),
            tool_calls: vec![ToolCall {
                id: "call_1".into(),
                name: "web.search".into(),
                args: r#"{"query":"papers"}"#.into(),
            }],
            ..Default::default()
        },
        WireMsg {
            role: "tool".into(),
            content: "<tool-result tool=\"web.search\" status=\"ok\">[]</tool-result>".into(),
            tool_call_id: Some("call_1".into()),
            ..Default::default()
        },
    ];

    let out = openai_msgs(&msgs);

    assert_eq!(out[0]["role"], "assistant");
    assert_eq!(out[0]["tool_calls"][0]["id"], "call_1");
    assert_eq!(out[0]["tool_calls"][0]["function"]["name"], "web_search");
    assert_eq!(
        out[0]["tool_calls"][0]["function"]["arguments"],
        r#"{"query":"papers"}"#
    );

    assert_eq!(out[1]["role"], "tool");
    assert_eq!(out[1]["tool_call_id"], "call_1");
    assert!(out[1]["content"].as_str().unwrap().contains("[]"));
}

#[test]
fn wire_names_sanitize_dots_for_provider_validation() {
    use argus_lib::gateway::adapters::{real_name, wire_name};

    assert_eq!(wire_name("fs.write"), "fs_write");
    assert_eq!(wire_name("computer.observe"), "computer_observe");
    assert_eq!(wire_name("terminal"), "terminal");
    assert_eq!(wire_name("grep"), "grep");
}

#[test]
fn real_name_round_trips_through_wire_names() {
    use argus_lib::gateway::adapters::{real_name, wire_name};
    use argus_lib::gateway::schema::ToolSpec;

    let specs = |names: &[&str]| {
        names
            .iter()
            .map(|n| ToolSpec {
                name: n.to_string(),
                description: String::new(),
                parameters: serde_json::json!({}),
            })
            .collect::<Vec<_>>()
    };
    let tools = specs(&["terminal", "fs.write", "browser.click"]);

    assert_eq!(real_name("terminal", &tools), "terminal");
    assert_eq!(real_name("fs_write", &tools), "fs.write");
    assert_eq!(real_name("browser_click", &tools), "browser.click");
    assert_eq!(real_name("nope_tool", &tools), "nope_tool");
    assert_eq!(wire_name(&real_name("fs_write", &tools)), "fs_write");
}

#[test]
fn every_shipped_tool_sanitizes_to_valid_wire_name() {
    use argus_lib::gateway::adapters::wire_name;

    let valid = regex::Regex::new(r"^[A-Za-z0-9_-]+$").expect("valid test regex must compile");
    for t in argus_lib::tools::tool_specs(true) {
        let wired = wire_name(&t.name);
        assert!(
            valid.is_match(&wired),
            "tool {} sanitizes to invalid wire name {}",
            t.name,
            wired
        );
    }
}

#[test]
fn openai_wire_tools_carry_sanitized_names() {
    use argus_lib::gateway::adapters::openai_compat::wire_tools;
    use argus_lib::gateway::schema::ToolSpec;

    let tools = vec![ToolSpec {
        name: "fs.write".into(),
        description: "d".into(),
        parameters: serde_json::json!({"type": "object"}),
    }];
    let out = wire_tools(&tools);

    assert_eq!(out[0]["type"], "function");
    assert_eq!(out[0]["function"]["name"], "fs_write");
    assert_eq!(out[0]["function"]["description"], "d");
}

#[test]
fn anthropic_wire_tools_carry_sanitized_names() {
    use argus_lib::gateway::adapters::anthropic::wire_tools;
    use argus_lib::gateway::schema::ToolSpec;

    let tools = vec![ToolSpec {
        name: "memory.save".into(),
        description: "d".into(),
        parameters: serde_json::json!({"type": "object"}),
    }];
    let out = wire_tools(&tools);

    assert_eq!(out[0]["name"], "memory_save");
    assert_eq!(out[0]["input_schema"]["type"], "object");
}
