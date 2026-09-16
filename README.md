# Argus

![version](https://img.shields.io/badge/version-0.1.0-blue)
![TypeScript](https://img.shields.io/badge/TypeScript-5.8-3178C6?logo=typescript&logoColor=white)
![React](https://img.shields.io/badge/React-19-61DAFB?logo=react&logoColor=black)
![Rust](https://img.shields.io/badge/Rust-1.0-CE422B?logo=rust&logoColor=white)
![Tauri](https://img.shields.io/badge/Tauri-2-FFC131?logo=tauri&logoColor=black)
![SQLite](https://img.shields.io/badge/SQLite-3-003B57?logo=sqlite&logoColor=white)
![license](https://img.shields.io/badge/license-MIT-green)

A personal AI agent that lives on your machine: chat, desktop control, skills, and fourteen connected services (Google, GitHub, Slack, Notion, Linear…).

## Features

- **Agent chat** — persistent sessions with permission modes (`ask` pauses writes for approval, `never` for trusted flows), tool streaming, and compacting summaries.
- **Computer use** — accessibility-tree grounding with snapshot tokens, screenshots with coordinate grounding, window management, and app launching on X11.
- **Browser use** — snapshot-ref clicking/typing/reading with stale-reference recovery, in your everyday Chrome via extension or isolated profiles.
- **Terminal & files** — shell execution with live output, recursive grep, file read/write.
- **Skills & memory** — reusable saved skills plus durable fact/preference/project memory.
- **Document library** — create docx, pdf, pptx, xlsx, csv, md, txt artifacts.
- **Local-first storage** — SQLite plus OS keyring; API keys never touch plain text.

## Quickstart

```bash
bun install
bunx tauri dev
```

Full walkthrough: [docs/getting-started/quickstart.md](docs/getting-started/quickstart.md).

## Model providers

Bring your own model. Argus speaks two provider dialects:

| Dialect | Examples |
|---|---|
| Anthropic | Anthropic API |
| OpenAI-compatible | OpenAI, Baseten, self-hosted Ollama, any OpenAI-style endpoint |

Setup: Settings → Providers → pick a provider → paste the API key and connect → enable models in Settings → Models. Keys live in the OS keyring, never in plain text.

## Integrations

Fourteen connectors with OAuth/device-flow setup under Settings → Connectors:

| Connector | Covers |
|---|---|
| Google Workspace | Gmail, Calendar, Drive, Docs, Sheets |
| GitHub | Issues, PRs, repos, Actions |
| GitLab | Projects, issues, merge requests |
| Slack | Workspaces, channels, messages |
| Notion | Pages, databases |
| Linear | Issues, projects |
| Outlook | Mail, calendar |
| Discord | Servers, channels, messages |
| Figma | Files, projects |
| Home Assistant | Devices, states, services |
| Spotify | Playback, playlists, library |
| Telegram | Chats, messages |
| Todoist | Tasks, projects |
| Trello | Boards, lists, cards |

Details per connector: [docs/user-guide/connectors.md](docs/user-guide/connectors.md).

## Docs

Start at [docs/README.md](docs/README.md) — user guides, connector setup, architecture, contributing, troubleshooting. Machine-readable index: [docs/llms.txt](docs/llms.txt).

## Stack

React + TypeScript frontend, Rust (Tauri) backend, SQLite + OS keyring. `cargo test` runs the backend suite; `npx tsc --noEmit` typechecks the frontend.

## License

MIT © 2026 Anyscape Labs. See [LICENSE](LICENSE).

## Thank you to our contributors

Argus exists because of the people who build it. Thank you!

<a href="https://github.com/BYRON-lang"><img src="https://github.com/BYRON-lang.png?size=64" width="64" height="64" alt="BYRON-lang" style="border-radius:50%" /></a>

Want to contribute? Start with the [contributing guide](docs/README.md) — issues and pull requests are welcome.
