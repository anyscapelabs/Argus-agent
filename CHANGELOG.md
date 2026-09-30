# Changelog

All notable changes to Argus. Versions follow semver; `-alpha.N` builds are
the private testing loop and are published as GitHub prereleases.

The format is based on [Keep a Changelog](https://keepachangelog.com/).

## [Unreleased]

Nothing yet. The next entry is `0.2.0-alpha.1`.

## [0.1.0] — Panoptes

The first build: a personal agent that lives on your machine.

### Added

- **Agent chat** — persistent sessions, `ask`/`never` permission modes,
  streaming tool output, compacting summaries.
- **Computer control** — terminal-first: streaming shell with cwd and
  configurable timeouts, recursive grep with file globs, files, processes,
  installs, and scripts.
- **Browser use** — snapshot-ref clicking, typing and reading with
  stale-reference recovery, in your everyday Chrome via extension or isolated
  profiles.
- **Skills and memory** — reusable saved skills, plus durable fact, preference
  and project memory.
- **Document library** — docx, pdf, pptx, xlsx, csv, md and txt artifacts.
- **Model providers** — Anthropic and any OpenAI-compatible endpoint (OpenAI,
  Baseten, self-hosted Ollama). Keys live in the OS keyring.
- **Fourteen integrations** — Google Workspace, GitHub, GitLab, Slack, Notion,
  Linear, Outlook, Discord, Figma, Home Assistant, Spotify, Telegram, Todoist,
  Trello.
- **Sandbox** — OS-level confinement (Landlock, seccomp, rlimits) for terminal
  execution, failing closed when a policy cannot be enforced.
- **Local-first storage** — SQLite plus the OS keyring; no plaintext secrets.

[Unreleased]: https://github.com/anyscapelabs/Argus-agent/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/anyscapelabs/Argus-agent/releases/tag/v0.1.0
