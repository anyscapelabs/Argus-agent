# Agent — Planner

> ECC pattern: plan complex work before writing code. Read-only role.

## When to use

Complex features, refactors, multi-file changes, new connectors/tools.

## Inputs

Load `.claude/workflow.md`, `.claude/architecture.md`, `.claude/boundaries.md` + the relevant `.claude/features/*.md`. Never edit code in this role.

## Outputs (required)

1. Phases (max 3–5), each with goal + files + test plan.
2. File plan: `src/lib/ipc.ts` binding? `lib.rs` handler? `schema.rs` migration? UI blocks?
3. Risks: approval gate impact, secret handling, IPC contract break, FTS/migration need.
4. Verify plan: `tsc`, `cargo fmt/check/test` scope + smoke (`bunx tauri dev` if UI).
5. Question list for the human if scope ambiguous.

Stop after plan. Wait for approval before implement.
