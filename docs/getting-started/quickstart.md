# Quickstart
> **Job:** Install Argus and hold the first conversation in minutes.

## Install

```bash
git clone https://github.com/anyscapelabs/Argus-agent
cd Argus-agent
bun install
bunx tauri dev
```

Release builds:

```bash
bun run build
```

## Connect a model provider

1. Open Settings → Providers.
2. Pick a provider (Anthropic, OpenAI, or any OpenAI-compatible endpoint).
3. Paste the API key and connect.
4. Enable the models you want in Settings → Models, then sync the catalog.

Keys live in the OS keyring, never in plain text.

## First chat

1. Press New Agent (or pick a session).
2. Type a task, e.g. `list the files in ~/Documents and summarize the biggest one`.
3. Sessions default to `ask` mode: writes pause for approval. Flip to `never` per session once you trust it.

Next: [Chat](../user-guide/chat.md), [Connectors](../user-guide/connectors.md).
