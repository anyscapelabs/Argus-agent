# Boundaries — What to Touch / Never Touch

Source: `docs/developer-guide/contributing.md`, `.gitignore`, `src-tauri/.gitignore`.

## SHOULD touch (normal feature work)

- `src/components/**/*.tsx`, `src/components/agent/*.tsx` — UI blocks/pages.
- `src/lib/ipc.ts` — add binding + types when adding a command. Keep `SCREAMING_SNAKE_CASE CMD_*` + `function` decls, `camelCase` values / `PascalCase` types.
- `src/stores/*.ts`, `src/hooks/*.ts`, `src/types/*.ts`, `src/index.css`, `src/App.tsx`, `src/main.tsx`.
- `src-tauri/src/{sessions,gateway,prompt,tools,skills,library,memory,connectors,mcp}/**/*.rs` + `src-tauri/src/lib.rs` handler list + `src-tauri/src/mcp/mod.rs: pub mod <service>`.
- `src-tauri/tests/*.rs` — ONLY test location (35 files today).
- `extension/{background.js,content.js,manifest.json}` — browser bridge only.
- `docs/**/*.md`, `docs/llms.txt`, `README.md`, `MANIFEST` — user-facing changes.
- Careful: `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`, `src-tauri/capabilities/default.json`, `vite.config.ts`, `index.html`, `tsconfig*.json`.

## MUST NOT touch / never commit

- `node_modules/`, `dist/`, `dist-ssr/`, `*.local`, `logs/`, `*.log` — build artifacts (gitignored).
- `bun.lock`, `src-tauri/Cargo.lock` — generated lockfiles; only via `bun install` / `cargo`, never hand-edit.
- `src-tauri/target/**`, `src-tauri/gen/schemas/**` — generated compile output / Tauri schemas.
- `.git/` — no history rewrites, no force-push.
- Secrets: `src-tauri/src/mcp/*/config.json` (see `config.example.json` templates), `argus.db`, OS keyrings (`argus-gw`, `argus-connector`, `argus-google`, `argus-github`), app-data (`skills/`, `library/`, `logos/`, `browser-profiles/`, `native.sock`, `extension.enabled`). Lookup vault → env → app-data → dev file.
- Gitignored private files stay untracked — never `git add -f` an ignored file. This `AGENTS.md` + `.claude/` must stay secret-free.
- Never log `token=` / `key=` values; never write keys/tokens to chat/logs/repo; never delete others' connector cards without discussion; no status polls in `connector_logs`.
- Do not create inline test modules in `src-tauri/src/`; do not add `console.*` / `println!` to prod.

## Style gates (full in rules/)

- TS/React: see `.claude/rules/typescript.md`. Rust: see `.claude/rules/rust.md`. Security: see `.claude/rules/security.md`.
- `cargo fmt` required before push. `npx tsc --noEmit` must pass. `strict,noUnusedLocals/Params` on.
