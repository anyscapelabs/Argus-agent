use argus_lib::mcp::github::config::SCOPES;

#[test]
fn scopes_cover_code_and_actions() {
    assert!(SCOPES.contains("repo"));
    assert!(SCOPES.contains("workflow"));
    assert!(SCOPES.contains("read:user"));
}

#[test]
fn example_config_parses() {
    let cfg: serde_json::Value =
        serde_json::from_str(include_str!("../src/mcp/github/config.example.json")).unwrap();

    assert!(!cfg.get("client_id").unwrap().as_str().unwrap().is_empty());
}
