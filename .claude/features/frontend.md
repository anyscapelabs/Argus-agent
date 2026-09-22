# Frontend — React UI, Stores, Agent Blocks

Code: `src/{App.tsx,lib/ipc.ts,lib/agentXml.ts,stores/,hooks/,components/}`, `src/index.css`, `src/types/chat.ts`.

## Shell

- `App.tsx`: view state (`new-agent|chat|memory|skills|library|projects|connectors`) + `sessionStore` + `SettingsModal + DocViewer + Toasts`. `listen("sessions-changed")`.
- `stores/sessions.ts`: `SessionStore` class + `useSyncExternalStore`; `loadSessions, select, create, send, retry, stop, setPermission/Model/WebSearch, archive, remove, setVote, resolveApproval`; `onTurnStart/onTurnDone → notify`.
- `stores/toast.ts (toast.success/error)`, `workTimer.ts`, `docViewer.ts`.
- Styling: `index.css` (`@import "tailwindcss"`, `@font-face` Inter/AnthropicSerif) + utility classes (`bg-bg-primary` etc.). 2-space indent.

## IPC + parsing

- `lib/ipc.ts`: SOLE invoke boundary. Top-level `function`s, `SCREAMING_SNAKE_CASE CMD_*`, `camelCase` values / `PascalCase` types. Sections: gateway (43-118), sessions (152-287), connectors/vault (290-327), skills/memory (329-408), library (410-454), browser/ext (256-276).
- `lib/agentXml.ts`: `TAG_SCHEMA` 20+ tags + `tokenize/buildTree/parse` + MD→XML. `lib/notify.ts` (`notifyDone`), `lib/relativeTime.ts`.
- `buildTree` recovers from malformed markup: a new block open force-closes the current node, and a garbled close (`</…>` failing the tag regex) emits an empty-tag close that ends the open node — one bad tag degrades to one odd card, never swallows the rest of the reply.

## Components

- Shell: `Sidebar, Toolbar, Dropdown, Toasts, SessionList, NewAgentPage, ChatDetailPage, ChatInput, AgentBubble (parse(agentXml)), UserBubble`.
- Agent blocks (`components/agent/`): `ThinkingBlock, PlanBlock, ActionBlock, ToolActivity, TerminalBlock, DiffBlock, FileChip, FileGroup, PathChip, TableBlock, WebSearchGroup, EmailDraftCard, DocumentCard, MemoryRefChip, ApprovalBlock (Run/Deny), StreamingIndicator, AlertBanner, WorkSummary`.
- Knowledge: `MemoryPage, SkillsPage, SkillCard, LibraryPage, LibraryCard, ConnectorsPage, ConnectorCard, ProjectsPage, ProjectCard`, `settings/* (ProvidersPage, ModelsPage, ProviderConnectModal, ConnectedProviderList, SettingsSidebar)`.

## Rules for agents

- New backend command → new `ipc.ts` binding + types first, then UI. Never `invoke` with raw strings outside `ipc.ts`.
- Keep `agentXml.ts` TAG_SCHEMA in sync with `chat.rs` parser and `prompt` format rules.
- Typecheck: `npx tsc --noEmit` (strict, `noUnusedLocals/Params`). No `console.*` in prod.
