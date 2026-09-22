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

#[test]
fn google_config_prefers_env_over_files() {
    use argus_lib::mcp::google::config::load;

    let prev_id = std::env::var("ARGUS_GOOGLE_CLIENT_ID").ok();
    let prev_secret = std::env::var("ARGUS_GOOGLE_CLIENT_SECRET").ok();
    std::env::set_var("ARGUS_GOOGLE_CLIENT_ID", "env-test-id");
    std::env::set_var("ARGUS_GOOGLE_CLIENT_SECRET", "env-test-secret");

    let cfg = load().unwrap();
    assert_eq!(cfg.client_id, "env-test-id");
    assert_eq!(cfg.client_secret, "env-test-secret");

    match prev_id {
        Some(v) => std::env::set_var("ARGUS_GOOGLE_CLIENT_ID", v),
        None => std::env::remove_var("ARGUS_GOOGLE_CLIENT_ID"),
    }
    match prev_secret {
        Some(v) => std::env::set_var("ARGUS_GOOGLE_CLIENT_SECRET", v),
        None => std::env::remove_var("ARGUS_GOOGLE_CLIENT_SECRET"),
    }
}

#[test]
fn github_config_prefers_env_over_files() {
    use argus_lib::mcp::github::config::load;

    let prev = std::env::var("ARGUS_GITHUB_CLIENT_ID").ok();
    std::env::set_var("ARGUS_GITHUB_CLIENT_ID", "env-test-id");

    let cfg = load().unwrap();
    assert_eq!(cfg.client_id, "env-test-id");

    match prev {
        Some(v) => std::env::set_var("ARGUS_GITHUB_CLIENT_ID", v),
        None => std::env::remove_var("ARGUS_GITHUB_CLIENT_ID"),
    }
}

#[test]
fn trello_config_prefers_env_over_files() {
    use argus_lib::mcp::trello::config::load;

    let prev = std::env::var("ARGUS_TRELLO_KEY").ok();
    std::env::set_var("ARGUS_TRELLO_KEY", "env-test-key");

    let cfg = load().unwrap();
    assert_eq!(cfg.api_key, "env-test-key");

    match prev {
        Some(v) => std::env::set_var("ARGUS_TRELLO_KEY", v),
        None => std::env::remove_var("ARGUS_TRELLO_KEY"),
    }
}

#[test]
fn drive_query_builds_name_contains_with_escape() {
    use argus_lib::mcp::google::drive::drive_query;

    assert_eq!(
        drive_query("budget"),
        "name contains 'budget' and trashed = false"
    );
    assert_eq!(
        drive_query("John's notes"),
        "name contains 'John\\'s notes' and trashed = false"
    );
}
