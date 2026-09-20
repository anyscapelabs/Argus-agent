use argus_lib::tools;
use argus_lib::tools::connector::{exec, META};
use serde_json::json;

#[test]
fn meta_entries_are_wellformed() {
    assert!(META.len() >= 40);

    for t in META {
        assert!(t.name.contains('.'), "{} lacks a service prefix", t.name);
        assert!(!t.desc.trim().is_empty(), "{} lacks a description", t.name);

        let ex = serde_json::from_str::<serde_json::Value>(t.args)
            .unwrap_or_else(|e| panic!("{} args are not JSON: {e}", t.name));
        assert!(ex.is_object(), "{} args are not an object", t.name);
    }
}

#[test]
fn writes_are_flagged_mutating() {
    for t in META {
        let mutating = t.name.ends_with("send")
            || t.name.contains("create")
            || t.name.ends_with("comment")
            || t.name.contains("append")
            || t.name.contains("close")
            || t.name.contains("delete")
            || t.name.ends_with("turn");

        assert_eq!(t.mutating, mutating, "{} mutating flag wrong", t.name);
    }
}

#[test]
fn agent_catalog_sees_connector_tools() {
    assert!(tools::is_mutating("linear.create_issue"));
    assert!(tools::is_mutating("slack.send"));
    assert!(tools::is_mutating("ha.turn"));
    assert!(!tools::is_mutating("linear.issues"));
    assert!(!tools::is_mutating("gitlab.pipelines"));

    let specs = tools::tool_specs(false);
    assert!(specs.iter().any(|s| s.name == "linear.issues"));
    assert!(specs.iter().any(|s| s.name == "trello.create_card"));

    let section = tools::protocol_section();
    assert!(section.contains("linear.issues"));
    assert!(section.contains("ha.turn"));
}

#[tokio::test]
async fn dispatch_routes_required_arg_tools() {
    let cases = [
        ("linear.issue", "id"),
        ("linear.create_issue", "team_id"),
        ("linear.comment", "issue_id"),
        ("slack.history", "channel"),
        ("slack.send", "channel"),
        ("notion.query_db", "db_id"),
        ("notion.create_page", "parent_id"),
        ("notion.append_text", "block_id"),
        ("figma.file", "key"),
        ("figma.comment", "key"),
        ("discord.channels", "guild_id"),
        ("discord.send", "channel_id"),
        ("telegram.send", "chat_id"),
        ("todoist.create_task", "content"),
        ("todoist.close_task", "id"),
        ("gitlab.issues", "project_id"),
        ("gitlab.create_issue", "project_id"),
        ("ha.state", "entity_id"),
        ("ha.turn", "domain"),
        ("trello.lists", "board_id"),
        ("trello.create_card", "list_id"),
        ("trello.comment", "card_id"),
    ];

    for (name, key) in cases {
        let err = exec(name, &json!({})).await.unwrap_err();
        assert_eq!(err, format!("missing {key}"), "{name} routed wrong");
    }
}

#[tokio::test]
async fn dispatch_rejects_unknown_tool() {
    let err = exec("linear.nope", &json!({})).await.unwrap_err();

    assert_eq!(err, "unknown connector tool");
}
