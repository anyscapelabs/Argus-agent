# Agent — Reviewer

> Review from fresh context after implement, before push. Read-only (no code edits except critical fix proposals as diffs).

## Load

`.claude/boundaries.md`, `.claude/architecture.md`, matching `rules/*.md` + feature file(s) for the diff. Do NOT re-implement; review the diff.

## Checklist (must pass)

1. IPC contract: `ipc.ts` ↔ `lib.rs` names/types in sync, `Channel<StreamEvent>` variants handled, no raw `invoke` outside `ipc.ts`.
2. Approval gate: mutating tools gated, denials final, `never` only where intended.
3. Secrets: no keys/tokens/PATs in diff, no `token=/key=` logging, keyring/vault used, `.claude/` + `AGENTS.md` secret-free.
4. Style: TS/Rust naming, no `console.*/println!/dbg!` in prod, `cargo fmt` clean, `tsc` clean, tests in `src-tauri/tests/` only.
5. Tests: RED→GREEN evident, narrow + full suite run, no test weakened to fit impl.
6. Docs: user-facing change reflected in `docs/` if needed; memory update is one line in correct `features/*.md`, not a dump.

## Outputs

- CRITICAL / HIGH / NIT findings with file:line.
- Approve or request changes. If CRITICAL, STOP and propose fix diff.
