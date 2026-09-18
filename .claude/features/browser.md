# Browser — Isolated Profiles + Extension Bridge

Code: `src-tauri/src/tools/browser/{mod.rs,ext.rs,extpipe.rs}`, `src-tauri/src/sessions/{browser_import.rs,ext_install.rs}`, `extension/{manifest.json,background.js,content.js}`, `src/lib/ipc.ts:256-276`.

## Isolated (`browser/mod.rs`)

- `init(dir:browser-profiles), profile_dir, sanitize(), open, click, type_text, read, scroll, close, close_profile`.
- `url_guard` refuses credentialed URLs. `redact(), sensitive_note()` (login/checkout → further actions need approval), `sensitive(), sensitive_pats()`.
- Snapshot-ref click/type/read + stale-ref recovery (see `browser_refs_test, browser_recover_test, browser_shown_test`).

## Extension bridge (`ext.rs`, `extpipe.rs`, `extension/`)

- `ext.rs`: `is_real(), open, read, click, type_text, scroll, close, sensitive()` for real Chrome.
- `extpipe.rs`: `connected(), start_listener(data_dir), serve_conn(UnixStream native.sock), request(method,data), run_stdio_host(socket)` (`--native-host` in `lib.rs:20`).
- `manifest.json`: MV3 `Argus Browser Bridge v1.0.0`, `permissions:[nativeMessaging,scripting,tabs]`, `host_permissions:[<all_urls>]`, `minimum_chrome_version:110`.
- `background.js`: native-messaging bridge `com.argus.browser`, req/resp frames, `connectNative`, ping 20s, nav timeout 20s, settle 2.5s, reconnect.
- `content.js`: `window.__argusSnapshot()` DOM snapshot ≤100 interactive els, CSS selector paths, never reads/fills password fields.

## Session helpers

- `browser_import.rs` (`sess_browser_import`, emits `browser-import-progress|done`), `ext_install.rs` (`sess_ext_install, sess_ext_status, sess_ext_uninstall, install_core`, `extension.enabled` sentinel, `native.sock`).
- Frontend: `sessBrowserImport, sessExtInstall/Uninstall/Status`; `ConnectorsPage.tsx:97 listen("browser-import-done")`.

## Rules for agents

- Never read/fill passwords, never type payments. Credentialed URLs refuse before load. Login/checkout flows always re-approve.
- Keep stale-ref recovery; do not replace snapshot-refs with raw selectors.
- Tests: `agent_real_browser_*`, `browser_ext_test, browser_refs_test, browser_recover_test, browser_verify_test`.
