# Adding connectors
> **Job:** The repeatable pattern behind every service in `src-tauri/src/mcp/`.

## Token services (Slack, Notion, Linear…)

1. New folder `src-tauri/src/mcp/<service>/` with `mod.rs` exposing async fns that take plain args and return `Result<serde_json::Value, String>`.
2. Read the token with `crate::mcp::vault::get("<service>")`; missing token errors as `<service> not connected`.
3. Add `pub mod <service>;` to `src-tauri/src/mcp/mod.rs`.
4. Cover offline-safe behavior in `src-tauri/tests/` (config parsing, pure helpers). No test modules inside `src/`.

Settings beyond a token (base URL, API key) go in `<service>/config.rs` with a committed `config.example.json` and a gitignored `config.json`. Lookup order: shared vault → env vars → app-data file → dev file.

## OAuth services (Google, GitHub, Outlook, Spotify)

Same as above, plus a flow module: loopback-callback server (Google, Spotify) or device-code polling (GitHub, Outlook). Refresh tokens live in the keyring; in-memory access tokens expire early and refresh on demand. Log `oauth_started` / `oauth_finished` via `connectors::log`.

## Frontend

Add `*_status` / `*_connect*` / `*_disconnect` bindings in `src/lib/ipc.ts`, a card on the Connectors page following `GoogleCard` (OAuth) or the token-paste pattern, and remove nothing else's card without discussion.

## Event logging

Log lifecycle events and API outcomes with `connectors::log::event` / `log::api`. Never log secret values — the logger redacts `token=`/`key=` itself, and status polls stay out of the table.
