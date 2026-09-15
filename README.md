# Argus

A personal AI agent that lives on your machine: chat, desktop control, skills, and fourteen connected services (Google, GitHub, Slack, Notion, Linear…).

## Quickstart

```bash
bun install
bunx tauri dev
```

Full walkthrough: [docs/getting-started/quickstart.md](docs/getting-started/quickstart.md).

## Docs

Start at [docs/README.md](docs/README.md) — user guides, connector setup, architecture, contributing, troubleshooting. Machine-readable index: [docs/llms.txt](docs/llms.txt).

## Stack

React + TypeScript frontend, Rust (Tauri) backend, SQLite + OS keyring. `cargo test` runs the backend suite; `npx tsc --noEmit` typechecks the frontend.
