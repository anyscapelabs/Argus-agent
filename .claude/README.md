# .claude/ — Argus Agent Memory

## Index

| File | Job | When to load |
|---|---|---|
| `architecture.md` | Layout, IPC, agent loop, storage, decisions | Any backend/frontend cross-cutting task |
| `workflow.md` | How to think, plan, TDD, implement, review, verify | Every code task |
| `boundaries.md` | Touch / never-touch, style, secrets | Every code task |
| `features/chat.md` | Sessions, approvals, streaming | Chat UI or `sessions/chat.rs` |
| `features/gateway.md` | Providers, models, router, keyring | Providers/models work |
| `features/memory.md` | Skills + durable memory + session-memory graph | Skills/Memory pages |
| `features/tools.md` | Terminal, grep, fs, notepad, web, recover | Computer-control tools |
| `features/browser.md` | Isolated profiles + extension bridge | Browser automation |
| `features/connectors.md` | 14 MCP connectors, vault, OAuth/device/PAT | Any integration |
| `features/library.md` | Document generation (docx/pdf/pptx/xlsx…) | Library artifacts |
| `features/prompt.md` | System prompt assembly + compaction | Prompt/context work |
| `features/frontend.md` | React stores, components, agent blocks | Any `src/` UI work |
| `rules/typescript.md` | TS/React conventions | Touching `src/` |
| `rules/rust.md` | Rust/Tauri conventions | Touching `src-tauri/src/` |
| `rules/security.md` | Approvals, keyring, redaction, injection | Mutating tools, secrets, web |
| `agents/planner.md` | Breaks work into phases + file plan | Complex features |
| `agents/backend.md` | Implements Rust commands + tests | Backend tasks |
| `agents/frontend.md` | Implements React UI + ipc.ts | Frontend tasks |
| `agents/reviewer.md` | Fresh-context review, checklist | After implement, before push |

## How to add memory

- One fact → one line in the correct `features/*.md`, with file path + line hint.
- New pattern → `rules/*.md` if always-applicable, else feature file.
- Repeated win → propose a skill (not yet scaffolded); do not duplicate docs.
- Never store secrets, tokens, PATs, or `config.json` contents here.
