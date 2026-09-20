use argus_lib::tools;
use argus_lib::tools::conn_oauth;
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

#[test]
fn oauth_meta_entries_are_wellformed() {
    assert!(conn_oauth::META.len() >= 35);

    for t in conn_oauth::META {
        assert!(t.name.contains('.'), "{} lacks a service prefix", t.name);
        assert!(!t.desc.trim().is_empty(), "{} lacks a description", t.name);

        let ex = serde_json::from_str::<serde_json::Value>(t.args)
            .unwrap_or_else(|e| panic!("{} args are not JSON: {e}", t.name));
        assert!(ex.is_object(), "{} args are not an object", t.name);
    }
}

#[test]
fn oauth_writes_are_flagged_mutating() {
    for t in conn_oauth::META {
        let mutating = t.name.ends_with("send")
            || t.name.ends_with("comment")
            || t.name.starts_with("gcal.create")
            || t.name.starts_with("gcal.delete")
            || t.name.starts_with("gdrive.create")
            || t.name.starts_with("gdrive.delete")
            || t.name.starts_with("gdocs.create")
            || t.name.starts_with("gdocs.append")
            || t.name.starts_with("gsheets.update")
            || t.name.starts_with("gsheets.append")
            || t.name.starts_with("gsheets.create")
            || t.name.contains("create_")
            || t.name.contains("close")
            || t.name.contains("merge")
            || t.name.contains("rerun")
            || t.name == "spotify.control";

        assert_eq!(t.mutating, mutating, "{} mutating flag wrong", t.name);
    }
}

#[test]
fn agent_catalog_sees_oauth_tools() {
    assert!(tools::is_mutating("gmail.send"));
    assert!(tools::is_mutating("github.merge_pr"));
    assert!(!tools::is_mutating("gdrive.read"));
    assert!(!tools::is_mutating("spotify.now_playing"));

    let specs = tools::tool_specs(false);
    assert!(specs.iter().any(|s| s.name == "gmail.search"));
    assert!(specs.iter().any(|s| s.name == "gsheets.update"));

    let section = tools::protocol_section();
    assert!(section.contains("gmail.send"));
    assert!(section.contains("gcal.create_event"));
}

#[tokio::test]
async fn dispatch_routes_oauth_required_arg_tools() {
    let cases = [
        ("gmail.read", "id"),
        ("gmail.send", "to"),
        ("gcal.create_event", "summary"),
        ("gdrive.read", "file_id"),
        ("gdrive.create", "name"),
        ("gdrive.delete", "file_id"),
        ("gdocs.read", "doc_id"),
        ("gdocs.append", "doc_id"),
        ("gsheets.values", "id"),
        ("gsheets.update", "id"),
        ("gsheets.create", "title"),
        ("github.repo", "full_name"),
        ("github.create_issue", "full_name"),
        ("github.comment", "full_name"),
        ("github.close_issue", "full_name"),
        ("github.create_pr", "full_name"),
        ("github.merge_pr", "full_name"),
        ("github.rerun_failed", "full_name"),
        ("outlook.send", "to"),
        ("outlook.create_event", "subject"),
        ("spotify.control", "action"),
    ];

    for (name, key) in cases {
        let err = conn_oauth::exec(name, &json!({})).await.unwrap_err();
        assert_eq!(err, format!("missing {key}"), "{name} routed wrong");
    }
}

#[tokio::test]
async fn dispatch_rejects_unknown_oauth_tool() {
    let err = conn_oauth::exec("gmail.nope", &json!({}))
        .await
        .unwrap_err();

    assert_eq!(err, "unknown connector tool");
}

#[tokio::test]
async fn dispatch_rejects_non_array_rows() {
    let err = conn_oauth::exec(
        "gsheets.update",
        &json!({"id":"x","range":"A1","values":"no"}),
    )
    .await
    .unwrap_err();

    assert_eq!(err, "missing values as an array of rows");
}
