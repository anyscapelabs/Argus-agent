# Rule — Rust / Tauri (always load when touching src-tauri/src/)
> **Job:** Naming, layout, and shape law for `src-tauri/src/`.

Source: `docs/developer-guide/contributing.md`, `src-tauri/Cargo.toml`.

## Naming

- Values, functions, and modules in `snake_case`; types and traits in `PascalCase` with no `I` prefix; consts and statics in `SCREAMING_SNAKE_CASE`.
- Error enums read `XxxError`, never bare `Err` (it collides visually with `Result::Err`). Expose a crate-level `pub type Result<T> = std::result::Result<T, XxxError>`.
- Names stay short but meaningful, drawn from one shared vocabulary: `usr tok id auth buf sz pl tmp req res conn ctx addr cfg opts def idx cnt chk init err msg tx blk prev hdr`.
- Files use `snake_case.rs`.

## Layout

- 4-space indent, 100 columns, same-line braces, `rustfmt` clean. `cargo fmt` is REQUIRED before push.
- Import order: `std`, blank line, external crates, blank line, crate-local.
- One blank line between the beats of a function — guard clauses, setup, core logic, return. Never two blank lines in a row, never a blank line as the first or last line inside a block.
- Run the formatter; naming and shape are the job here, not whitespace fights.

## Shape

- Guard first: check failure and return `Err` early, never `else` after a return or `?`. Keep functions flat and short.
- Propagate errors with `?` to the caller that can handle them — handle each error once, never log and return the same error. Never collapse a typed error into a `bool`; the caller must know why it failed.
- Deliberate panics (`.expect()`) only for invariants the program cannot survive, such as bad config at boot.

## Commentary

- Silence is golden: no comments on obvious code. Comment only edge cases, workarounds, and failures, in short blunt words: `Sorted`, `Make a plan`, `Drop it`, `Is it there?`, `Loud fail`, `Not my problem`, `Still cooking`.

## Strictness and boundaries

- No panics in prod paths: return `Result`, use `let-else` guards, keep functions flat. No `println!` / `eprintln!` / `dbg!` in prod — tests only.
- Tests ONLY in `src-tauri/tests/*.rs`. Never inline `#[cfg(test)]` in `src/` files.
- New command: `#[tauri::command]` fn + entry in `lib.rs generate_handler!` + `ipc.ts` binding + test in `tests/`.
- Single-writer SQLite: `Mutex<Connection>` in `Gateway`. Migrations in `*/schema.rs`. FTS5 + triggers for search. Parameterized queries only.
- Secrets via `keyring` / `mcp/vault.rs`, never in DB/logs/repo. Redact `token=/key=` in `connectors/log.rs` and browser `redact()`.
