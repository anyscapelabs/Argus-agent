# Rule — TypeScript / React (always load when touching src/)
> **Job:** Naming, layout, and shape law for `src/`.

Source: `docs/developer-guide/contributing.md`, `tsconfig.json`.

## Naming

- Values and functions in `camelCase`, types and interfaces in `PascalCase`, constants and statics in `SCREAMING_SNAKE_CASE` (`TAG_MAP`, `LOGO_CACHE`, `CMD_*`). No snake_case in TypeScript values.
- Error classes read `XxxError extends Error`. Interface and type names carry no `I` prefix.
- Names stay short but meaningful, drawn from one shared vocabulary: `usr tok id auth buf sz pl tmp req res conn ctx addr cfg opts def idx cnt chk init err msg tx blk prev hdr`. Casing follows the rule above (`usrId`, `authTok`, `chkTok`).
- Files use `kebab-case.ts`. Top-level declarations use the `function` keyword (arrow syntax for closures only).

## Layout

- 2-space indent, semicolons required, 80 columns, same-line braces.
- Import order: external packages first, blank line, then internal and relative imports.
- One blank line between the beats of a function — guard clauses, setup, core logic, return. Never two blank lines in a row, never a blank line as the first or last line inside a block.
- Run the formatter; naming and shape are the job here, not whitespace fights.

## Shape

- Guard first: validate and throw early, never nest behind `else` after a return or throw.
- Keep functions flat and short (<50 lines). Enter, do work, finish.

## Commentary

- Silence is golden: no comments on obvious code. Comment only edge cases, workarounds, and failures, in short blunt words: `Sorted`, `Make a plan`, `Drop it`, `Is it there?`, `Loud fail`, `Not my problem`, `Still cooking`.

## Strictness and boundaries

- Strict: `strict, noUnusedLocals, noUnusedParams, noFallthroughCases`, `jsx: react-jsx`, `moduleResolution: bundler`. `npx tsc --noEmit` must pass.
- All backend access via `src/lib/ipc.ts` wrappers. Never raw `invoke` strings in components. Keep `snake_case` command names + `camelCase` args in sync with `src-tauri/src/lib.rs`.
- Tailwind utilities + `src/index.css` tokens. No `console.*` in prod.
- State: `zustand` custom `ExternalStore` pattern in `src/stores/` (see `sessions.ts`). Side-effects in stores, not render.
- Agent XML: update `src/lib/agentXml.ts TAG_SCHEMA` + parser when prompt format changes.
