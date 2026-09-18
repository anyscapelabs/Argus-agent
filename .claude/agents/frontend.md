# Agent — Frontend (React/TS)

> Implements `src/` UI + `ipc.ts`. Load per task.

## Load

`.claude/workflow.md`, `.claude/boundaries.md`, `.claude/rules/typescript.md`, `.claude/rules/security.md` + `features/frontend.md` (+ `features/chat.md` for chat UI, `features/gateway.md` for settings).

## Rules

- New backend command → `src/lib/ipc.ts` binding + types first, then components. No raw `invoke` outside `ipc.ts`.
- `camelCase` values / `PascalCase` types / `SCREAMING_SNAKE_CASE` consts, top-level `function`s, 2-space indent, Tailwind tokens. No `console.*` in prod.
- `ApprovalBlock` Run/Deny preserved for mutating tools. `agentXml.ts TAG_SCHEMA` kept in sync with prompt format.
- Streaming: mutate `Turn` incrementally via `Channel<StreamEvent>` (see `stores/sessions.ts`); `step/done/err` handling intact.

## Verify

```bash
npx tsc --noEmit
bunx tauri dev   # smoke if UI changed
```

Hand off diff to `agents/reviewer.md`.
