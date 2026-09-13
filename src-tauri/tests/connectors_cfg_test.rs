use argus_lib::mcp::github::config::SCOPES as GH_SCOPES;
use argus_lib::mcp::google::config::SCOPES as GOOG_SCOPES;
use argus_lib::mcp::outlook::config::SCOPES as OUT_SCOPES;
use argus_lib::mcp::spotify::config::SCOPES as SPOT_SCOPES;

#[test]
fn service_examples_parse() {
    for raw in [
        include_str!("../src/mcp/trello/config.example.json"),
        include_str!("../src/mcp/gitlab/config.example.json"),
        include_str!("../src/mcp/ha/config.example.json"),
        include_str!("../src/mcp/outlook/config.example.json"),
        include_str!("../src/mcp/spotify/config.example.json"),
        include_str!("../src/mcp/github/config.example.json"),
        include_str!("../src/mcp/google/config.example.json"),
    ] {
        serde_json::from_str::<serde_json::Value>(raw).unwrap();
    }
}

#[test]
fn oauth_scopes_cover_claimed_services() {
    assert!(GOOG_SCOPES.join(" ").contains("gmail.modify"));
    assert!(GH_SCOPES.contains("repo"));
    assert!(OUT_SCOPES.contains("Mail.ReadWrite"));
    assert!(OUT_SCOPES.contains("offline_access"));
    assert!(SPOT_SCOPES.contains("user-modify-playback-state"));
}
