//! MCP client layer against a mock stdio server — a POSIX sh script that
//! answers the three methods the client speaks. One shared lock serializes
//! tests because the registry is process-global.

use std::sync::{Mutex, OnceLock};

use argus_lib::connectors::Connector;
use argus_lib::mcp;
use serde_json::json;

fn lock() -> std::sync::MutexGuard<'static, ()> {
    static L: OnceLock<Mutex<()>> = OnceLock::new();
    L.get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|p| p.into_inner())
}

/// A mock MCP server: echoes the request id back, serves fixed tools.
fn mock_script() -> String {
    r#"#!/bin/sh
while IFS= read -r line; do
  id=$(printf '%s' "$line" | sed -n 's/.*"id":\([0-9]*\).*/\1/p')
  case "$line" in
    *'"initialize"'*)
      printf '{"jsonrpc":"2.0","id":%s,"result":{"protocolVersion":"2024-11-05","capabilities":{"tools":{}},"serverInfo":{"name":"mock","version":"0"}}}\n' "$id" ;;
    *'"tools/list"'*)
      printf '{"jsonrpc":"2.0","id":%s,"result":{"tools":[{"name":"echo","description":"Echo tool","inputSchema":{"type":"object"}}]}}\n' "$id" ;;
    *'"tools/call"'*)
      case "$line" in
        *'"die"'*)
          printf '{"jsonrpc":"2.0","id":%s,"result":{"content":[{"type":"text","text":"bye"}]}}\n' "$id"
          exit 0 ;;
        *'"env"'*)
          printf '{"jsonrpc":"2.0","id":%s,"result":{"content":[{"type":"text","text":"marker=%s scrub=%s"}]}}\n' "$id" "$MCP_MARKER" "$SCRUB_ME" ;;
        *'"fail"'*)
          printf '{"jsonrpc":"2.0","id":%s,"result":{"content":[{"type":"text","text":"nope"}],"isError":true}}\n' "$id" ;;
        *)
          printf '{"jsonrpc":"2.0","id":%s,"result":{"content":[{"type":"text","text":"hello from mock"}]}}\n' "$id" ;;
      esac ;;
  esac
done
"#
    .to_string()
}

fn mock_path() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("argus-mcp-mock-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let script = dir.join("mcp-mock.sh");
    std::fs::write(&script, mock_script()).unwrap();

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    script
}

fn configure(id: &str, env: Vec<(String, String)>) {
    mcp::configure(vec![Connector {
        id: id.into(),
        command: "/bin/sh".into(),
        args: vec![mock_path().to_string_lossy().into_owned()],
        env,
    }]);
}

#[tokio::test]
async fn handshake_tools_list_and_call() {
    let _g = lock();
    configure("mock-a", vec![]);

    let tools = mcp::tools("mock-a").await.unwrap();
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0].name, "echo");
    assert_eq!(tools[0].desc, "Echo tool");

    let out = mcp::call("mock-a", "echo", &json!({})).await.unwrap();
    assert_eq!(out, "hello from mock");
}

#[tokio::test]
async fn tool_level_error_surfaces_without_respawn() {
    let _g = lock();
    configure("mock-b", vec![]);

    let err = mcp::call("mock-b", "fail", &json!({})).await.unwrap_err();
    assert_eq!(err, "nope");
}

#[tokio::test]
async fn respawn_after_server_death() {
    let _g = lock();
    configure("mock-c", vec![]);

    // the server answers, then exits — this call still succeeds
    let out = mcp::call("mock-c", "die", &json!({})).await.unwrap();
    assert_eq!(out, "bye");

    // the next call respawns the server and works
    let out = mcp::call("mock-c", "echo", &json!({})).await.unwrap();
    assert_eq!(out, "hello from mock");
}

#[tokio::test]
async fn env_is_scrubbed_but_config_env_passes() {
    let _g = lock();
    std::env::set_var("SCRUB_ME", "leaked");
    configure("mock-d", vec![("MCP_MARKER".into(), "secret42".into())]);

    let out = mcp::call("mock-d", "env", &json!({})).await.unwrap();
    assert_eq!(out, "marker=secret42 scrub=");
}

#[tokio::test]
async fn unknown_connector_is_rejected() {
    let _g = lock();
    mcp::configure(vec![]);

    assert!(mcp::call("nope", "x", &json!({})).await.is_err());
    assert!(mcp::tools("nope").await.is_err());
}
