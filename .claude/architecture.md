# Architecture — Argus (v0.1.0 Panoptes)

Source: `docs/developer-guide/architecture.md`, `src-tauri/src/lib.rs`, `MANIFEST`. Do not guess paths.

## Layout

```text
src/                  React 19 + TS 5.8 + Tailwind 4 (2-space indent)
  App.tsx             View router: new-agent|chat|memory|skills|library|projects|connectors + SettingsModal+DocViewer+Toasts
  main.tsx            Boot -> App.tsx (via index.html)
  lib/ipc.ts          SOLE frontend IPC boundary (top-level functions wrapping invoke)
  lib/agentXml.ts     <action>/<terminal>/<plan>/<thinking>/… tokenizer + tree + MD normalizer
  stores/sessions.ts  SessionStore (ExternalStore, Channel<StreamEvent> handling)
  stores/toast.ts, workTimer.ts, docViewer.ts
  hooks/useProviders.ts, useChatModels.ts, useModels.ts, useProviderLogo.ts
  components/         Sidebar, Toolbar, ChatDetailPage, ChatInput, AgentBubble, UserBubble, SessionList, …
  components/agent/   ActionBlock, TerminalBlock, ApprovalBlock, ThinkingBlock, ToolActivity, …
  components/settings/ ProvidersPage, ModelsPage, ProviderConnectModal, …
src-tauri/src/        Rust backend (rustfmt: 4 spaces, 100 cols)
  lib.rs              Tauri setup + ~70-command generate_handler! registry + app dirs (db, skills, library, logos, browser-profiles)
  main.rs             Calls argus_lib::run(); --native-host branch -> extpipe stdio host
  sessions/           chat.rs (12-step loop), store.rs, schema.rs, browser_import.rs, ext_install.rs
  gateway/            mod.rs, router.rs (failover stream), catalog.rs (models.dev sync), store.rs, schema.rs, adapters/
  prompt/             mod.rs (project assembly), config.rs (per-model windows), compressor.rs
  tools/              mod.rs (TOOLS dispatch + approval gate), shell/, fs/, grep/, web/, browser/, notepad.rs, recover.rs
  skills/ library/    CRUD + FTS5 + file sync
  memory/             memories + memory_links + session_memory graph
  connectors/         registry + vault pointer + connector_logs (redacted)
  mcp/                client.rs (stdio MCP), vault.rs (argus-connector), <14 services>/
src-tauri/tests/      35 integration test files — ONLY test location
extension/            Chrome MV3 bridge: manifest.json, background.js (nativeMessaging), content.js (__argusSnapshot)
```

## Agent loop (`sessions/chat.rs::send`, ≤12 steps)

1. Build project prompt (`prompt::project`: identity + format + tools + prefs + skill index + summary).
2. Stream model reply (`gateway::router::stream_run` via `Channel<StreamEvent>`).
3. Parse `<action>` blocks (`tools::parse_actions`).
4. Approval gate (`tools::is_mutating` + `permission ask/never`; blocked → `ApprovalBlock` Run/Deny).
5. Dispatch `tools::exec` → results back as `<tool-result>`.
6. Loop-guard: 3rd identical action trips guard. Screenshots by path attach as images (last 2 msgs). Overflow → `prompt::compressor` utility-model summarization.

## IPC boundary

- Frontend `invoke<Row>(snake_case_cmd, {camelCase args})` in `src/lib/ipc.ts` ↔ `#[tauri::command] snake_case_fn` in `src-tauri/src/lib.rs`.
- Streaming: `tauri::ipc::Channel<StreamEvent>` (`delta|reset|step|term|term_end|approval|notice|err`) — see `src/stores/sessions.ts:294-357`.
- Events: `app.emit("sessions-changed"|"browser-import-progress|done")` ↔ `listen()` in `App.tsx`, `ConnectorsPage.tsx`.
- Escape hatch: `--native-host` → `tools/browser/extpipe.rs::run_stdio_host(native.sock)`.

## Data (local-first)

- One SQLite `argus.db` in `app_data_dir` (`Mutex<Connection>` in `Gateway`). Tables: `sessions/messages/summaries/folders`, `skills+skills_fts+skill_stats`, `library`, `memories+memory_fts+memory_links+session_memory(+fts)`, `providers/models`, `connector registry + connector_logs`, kv prefs (`newagent.permission/model/webSearch`, `routing`).
- Files: skill `.md` two-way sync (`lib.rs:49-51`), library blobs (`lib.rs:53-54`), `logos/`, `browser-profiles/`.
- Secrets NEVER in DB/repo/logs: keyring `argus-gw` (providers), `argus-connector` (generic), `argus-google`, `argus-github` (OAuth refresh). Lookup order: vault → env → app-data file → dev file (see `docs/reference/environment.md`).

## Key decisions (do not relitigate without discussion)

- Tauri 2 + single-writer SQLite + OS keyring: offline-first, no server.
- Terminal-first computer control: shell/grep/fs go through `tools/` with cwd/timeout/streaming, not ad-hoc exec.
- Snapshot-ref browser (stale-ref recovery) over raw selectors; extension bridge over CDP for real Chrome.
- FTS5 (`porter`) + triggers for skills/memory search; petgraph only in memory graph paths.
- Compaction + titles run on cheapest enabled model (`prompt::compressor`, `catalog.rs`).
