# Workflow — How to Think and Write Code
> **Job:** Plan, test, implement, review, verify, remember.

## 1. Think (before touching code)

- Load `AGENTS.md` route + one `features/*.md` + matching `rules/*.md`. Do NOT load all memory files.
- For complex work, delegate to `agents/planner.md` first: produce phases, file plan, risk list, test plan. Get human approval on plan.
- Identify IPC impact: does `src/lib/ipc.ts` need a new binding? Does `src-tauri/src/lib.rs` handler list need a new `#[tauri::command]`? Keep `snake_case` commands / `camelCase` args.
- Identify permission impact: is the tool `mutating:true`? Then `ask` gate + `ApprovalBlock` path must be preserved (see `rules/security.md`).

## 2. Test-first (mandatory for backend, expected for UI logic)

- Tests live ONLY in `src-tauri/tests/*.rs`. Never `#[cfg(test)]` in `src/` files, never ad-hoc test files elsewhere.
- Find the closest existing test (e.g. `chat_test.rs`, `tool_exec_test.rs`, `memory_test.rs`) and mirror its setup.
- RED → GREEN → REFACTOR. Run the narrowest suite first, then full suite before push.

## 3. Implement (fast, flat, short, no noise)

- One feature file per task. Keep functions <50 lines, files focused (<800 lines). Prefer many small files by feature/domain.
- Guard first, never `else` after return/throw/`?`. Beats separated by one blank line: guards, setup, core, return.
- TS: `camelCase` values, `PascalCase` types, `SCREAMING_SNAKE_CASE` consts, top-level `function` decls, `XxxError` classes (see `rules/typescript.md`).
- Rust: `snake_case` values, `PascalCase` types, `XxxError` enum + crate `Result<T>`, `?` propagation (see `rules/rust.md`).
- Names short but meaningful from the shared vocabulary (`usr tok chk sz err buf req res ctx cfg opts tx blk msg`), casing per language.
- Comments only on edge cases and failures, in short blunt words: `Sorted`, `Drop it`, `Make a plan`, `Is it there?`, `Loud fail`, `Not my problem`, `Still cooking`.
- No `console.*` / `println!` / `eprintln!` / `dbg!` in prod paths — tests only.
- Run `npx tsc --noEmit` (frontend) and `cargo fmt` + `cargo check` (backend) incrementally.

## 4. Review (fresh context)

- After implement, hand off to `agents/reviewer.md`: full diff, style, IPC contract, approval gate, redaction, test coverage.
- Address CRITICAL/HIGH before push. Do not self-approve your own diff without reviewer pass on multi-file changes.

## 5. Verify (before declaring done)

```bash
npx tsc --noEmit          # root
cargo fmt --check         # src-tauri/ or root
cargo check               # src-tauri/
cargo test                # src-tauri/ (full suite)
bunx tauri dev            # smoke desktop loop if UI changed (devUrl http://localhost:1420)
```

- `cargo fmt` is REQUIRED before push (`docs/developer-guide/contributing.md:10`).
- If build fails → fix incrementally, verify after each fix. Do not edit tests to fit broken impl unless tests are provably wrong.

## 6. Remember (persist, don't duplicate)

- If the task produced docs/code comments in the right place, do NOT duplicate into `.claude/`.
- Else add ONE line to the correct `.claude/features/*.md` (path + decision). Ask before creating new top-level files.
- Personal/debug notes stay out of the repo.
