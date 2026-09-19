# Connectors — 14 MCP Integrations

Code: `src-tauri/src/connectors/{mod.rs,store.rs,log.rs}`, `src-tauri/src/mcp/{mod.rs,client.rs,vault.rs,<service>/}`, `src/components/{ConnectorsPage,ConnectorCard}.tsx`.

## Infra

- `connectors/mod.rs`: `Connector{id,command,args,env}`, `store::enabled/migrate`, `log::event/api + redact(token=/key=) + KEEP cap`, `connector_logs` table.
- `mcp/mod.rs`: `PROTOCOL_VERSION 2025-06-18`, `REGISTRY OnceLock<Mutex<HashMap>>`, `configure(), sync_from(conn)` (`lib.rs:47`), `tools(server), call(server,tool,args)` stdio spawn + auto-respawn + tail on error.
- `vault.rs`: `SERVICE="argus-connector"` + `conn_save_token, conn_has_token, conn_has_client, conn_remove_token, conn_save_client, conn_save_secret, conn_clear_client, conn_clear_secret` (`get()` errors `<service> not connected`). Lookup: vault → env → app-data file → dev file.
- Credential overrides: Settings → Connectors tab (`ConnectorsSettingsPage.tsx`) writes per-service token/client/secret to the vault; all 14 service config paths check vault first (trello `get_client`, github `get_secret` included).

## Services (backend dir → auth)

| # | Service | Dir | Auth |
|---|---|---|---|
| 1 | Google (Gmail/Calendar/Drive/Docs/Sheets) | `mcp/google/{mod,oauth,tokens,config,gmail,calendar,drive,docs,sheets}` + `google_status/connect_url/disconnect` | loopback `127.0.0.1` OAuth, refresh in `argus-google`, `ARGUS_GOOGLE_CLIENT_ID/SECRET` |
| 2 | GitHub (repos/issues/prs/actions) | `mcp/github/{mod,device,tokens,config,issues,prs,repos,actions}` + `github_status/connect/disconnect` | device-code poll, `argus-github`, `ARGUS_GITHUB_CLIENT_ID/SECRET` |
| 3 | GitLab | `mcp/gitlab/{mod,config}` | PAT, `ARGUS_GITLAB_URL` self-host |
| 4 | Slack | `mcp/slack/mod.rs` | `xoxb-` bot token in vault |
| 5 | Notion | `mcp/notion/mod.rs` | internal integration token (must share pages/DBs) |
| 6 | Linear | `mcp/linear/mod.rs` | personal API key GraphQL |
| 7 | Outlook | `mcp/outlook/{mod,device,config}` + `outlook_status/connect/disconnect` | Entra device-code, `ARGUS_OUTLOOK_CLIENT_ID` |
| 8 | Discord | `mcp/discord/mod.rs` | bot token + Message Content intent |
| 9 | Figma | `mcp/figma/mod.rs` | PAT |
| 10 | Home Assistant | `mcp/ha/{mod,config}` | `base_url` + LLAT, `ARGUS_HA_URL` |
| 11 | Spotify | `mcp/spotify/{mod,oauth,config}` + `spotify_status/connect_url/disconnect` | loopback OAuth, `ARGUS_SPOTIFY_CLIENT_ID` |
| 12 | Telegram | `mcp/telegram/mod.rs` | BotFather token |
| 13 | Todoist | `mcp/todoist/mod.rs` | token |
| 14 | Trello | `mcp/trello/{mod,config}` | key+token, `ARGUS_TRELLO_KEY` |

Full setup per service: `docs/user-guide/connectors.md`. Adding one: `docs/developer-guide/adding-connectors.md` (token vs OAuth pattern: `mcp/<svc>/mod.rs + vault::get + pub mod + tests/`, `config.rs + config.example.json`, frontend `ipc.ts` + card).

## Rules for agents

- Never paste/log PATs, bot tokens, client secrets. Use vault/env only. Redact `token=/key=` in logs.
- Frontend: generic vault/connectors IPC + `ConnectorCard` pattern; Google/GitHub have dedicated `ipc.ts:290-327` bindings.
- Tests: `src-tauri/tests/connectors_test.rs, connectors_cfg_test.rs, github_oauth_test.rs, google_oauth_test.rs, mcp_test.rs`.
