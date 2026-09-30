# Changelog

All notable changes to Argus. Versions follow semver; `-alpha.N` builds are
the private testing loop and are published as GitHub prereleases.

The format is based on [Keep a Changelog](https://keepachangelog.com/).

## [Unreleased]

Nothing yet. The next entry is `0.3.0`, the public release.

## [0.2.0-alpha.1] — Sphinx

The first alpha of the testing loop. Everything below either ships as a build
or is documentation for it; the public release is 0.3.0.

### Added

- **Signed release builds for every platform** — macOS (Apple Silicon and
  Intel), Windows, and Linux (`.deb` and AppImage), triggered by pushing a
  version tag. Alpha builds are self-signed, so expect a platform warning on
  first launch; see `docs/developer-guide/releasing.md`.
- **CI on every push and pull request** — `cargo fmt --check`, clippy with
  `-D warnings`, the full backend suite, `tsc --noEmit`, `bun test`, and a
  production frontend build. The version is checked for consistency across
  `package.json`, `tauri.conf.json`, `Cargo.toml`, and the README badge.
- **Scrollable sidebar** — the chat list scrolls independently of the window,
  with the same thumb as the model list.
- **Release documentation** — a changelog, a manifest with the version code
  name, and a guide covering the version scheme, what signing actually gets
  you, and how to move to real certificates.

### Removed

- **Voice input** — local dictation shipped and was removed before this alpha.
  A transcript is indistinguishable from typed text, so a misheard instruction
  becomes an executed one, and the read-back half wanted cloud text-to-speech,
  which breaks the offline promise. Possible v0.4 feature, not v0.2.

[Unreleased]: https://github.com/anyscapelabs/Argus-agent/compare/v0.2.0-alpha.1...HEAD
[0.2.0-alpha.1]: https://github.com/anyscapelabs/Argus-agent/compare/v0.1.0...v0.2.0-alpha.1

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
