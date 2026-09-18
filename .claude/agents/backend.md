# Agent — Backend (Rust/Tauri)

> Implements `src-tauri/` commands + tests. Load per task.

## Load

`.claude/workflow.md`, `.claude/boundaries.md`, `.claude/architecture.md`, `.claude/rules/rust.md`, `.claude/rules/security.md` + one of `features/{chat,gateway,memory,tools,browser,connectors,library,prompt}.md`.

## Rules

- New command → `#[tauri::command]` + `lib.rs` registry + `src/lib/ipc.ts` binding + `src-tauri/tests/*.rs` test. Keep names `snake_case` / args `camelCase`.
- `Result` + `let-else`, no panics, no `println!/dbg!` in prod. `cargo fmt` before done.
- Mutating tool → `mutating:true` + approval-gate test. Secrets via keyring/vault only, redact `token=/key=`.
- Migrations in `*/schema.rs`; FTS triggers kept in sync; parameterized queries.

## Verify

```bash
cargo fmt --check
cargo check
cargo test
```

Hand off diff to `agents/reviewer.md` for multi-file changes.
