# Architecture
> **Job:** Map of the codebase for contributors.

## Layout

```text
src/                  React + TypeScript frontend (2-space indent)
  components/         Chat, settings, connectors, agent bubbles
  lib/ipc.ts          All Tauri command bindings (top-level functions)
  stores/sessions.ts  Session store
src-tauri/src/        Rust backend (rustfmt: 4 spaces, 100 cols)
  lib.rs              Tauri setup, command registry, app directories
  sessions/           Chat loop (chat.rs), sessions/folders store, browser import, ext install
  gateway/            Providers, model catalog, router with failover, keyring secrets
  prompt/             System prompt assembly, per-model context windows, compaction
  tools/              terminal, grep, fs, web, browser/*
  skills/ library/    CRUD + file sync
  connectors/         Connector registry, shared token vault, connector_logs
  mcp/                External MCP stdio clients + service modules (google, github, slack…)
src-tauri/tests/      Integration tests (all tests live here, none in src/)
```

## Agent loop

`sessions/chat.rs::send` runs up to 12 steps: project prompt → stream model reply → parse `<action>` blocks → approval gate → `tools::exec` dispatch → results back in as `<tool-result>`. Identical actions trip a loop guard on the third repeat. Screenshots referenced by path attach as images (last two messages). Context overflow triggers `prompt::compressor` summarization with a utility model.

## Prompt assembly

`prompt::project` builds the system message: base identity + reply-format rules, tool section, user preferences, skill index, and the latest compaction summary. Compaction and titles run on the cheapest enabled model.

## Data

One SQLite file (`argus.db` in app data): sessions, messages, summaries, skills, library, connectors, key-value prefs, `connector_logs`. Secrets stay in the OS keyring.
