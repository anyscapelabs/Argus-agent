use argus_lib::mcp::google::config::SCOPES;
use argus_lib::mcp::google::oauth::callback_query;

fn param(q: &[(String, String)], k: &str) -> Option<String> {
    q.iter().find(|(key, _)| key == k).map(|(_, v)| v.clone())
}

#[test]
fn callback_query_decodes() {
    let q = callback_query("/callback?code=4%2Fabc&state=x-y");

    assert_eq!(param(&q, "code").as_deref(), Some("4/abc"));
    assert_eq!(param(&q, "state").as_deref(), Some("x-y"));
    assert!(callback_query("/callback").is_empty());
}

#[test]
fn scopes_cover_all_five_services() {
    let joined = SCOPES.join(" ");

    for s in [
        "gmail.modify",
        "/auth/calendar",
        "/auth/drive",
        "/auth/documents",
        "/auth/spreadsheets",
    ] {
        assert!(joined.contains(s), "missing {s}");
    }
}

#[test]
fn example_config_parses() {
    let cfg: serde_json::Value =
        serde_json::from_str(include_str!("../src/mcp/google/config.example.json")).unwrap();

    assert!(!cfg.get("client_id").unwrap().as_str().unwrap().is_empty());
}
