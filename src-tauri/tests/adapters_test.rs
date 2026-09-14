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
    assert_eq!(out[0]["tool_calls"][0]["function"]["name"], "web.search");
    assert_eq!(
        out[0]["tool_calls"][0]["function"]["arguments"],
        r#"{"query":"papers"}"#
    );

    assert_eq!(out[1]["role"], "tool");
    assert_eq!(out[1]["tool_call_id"], "call_1");
    assert!(out[1]["content"].as_str().unwrap().contains("[]"));
}
