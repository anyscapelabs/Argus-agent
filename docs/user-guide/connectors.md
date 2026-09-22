# Connectors
> **Job:** Connect Argus to outside services, and what the agent can do with each.

Connectors live on the Connectors page. Two auth shapes exist:

- **OAuth / device flow** (Google, GitHub, Outlook, Spotify): click Connect, approve in the browser, the card flips to connected. Tokens refresh themselves.
- **Token paste** (everything else): create the token on the service's site, paste it in, Argus verifies and stores it in the OS keyring.

Bring your own credentials everywhere: Argus ships no shared keys. Client IDs and tokens live in your keyring, environment, or `*-settings.json` files — never in the repo.

Every connector event (connect, disconnect, API errors) lands in the local `connector_logs` table for inspection.

## What the agent can do

Once a connector is connected, the agent calls it like any other tool — ask in chat and it fetches data or acts. Read tools run freely; write tools (send, create, comment, close, delete, merge, smart-home `turn`) pause for approval in `ask` permission mode, and every call is logged with token/key values redacted.

- **Linear**: issues, teams, create issue, comment
- **Slack**: channels, history, threads, send, users
- **Notion**: search, query databases, read pages/blocks, create page, append text
- **Figma**: file meta, comments, post comment
- **Discord**: servers, channels, history, send
- **Exa**: AI web search with snippets, full page text
- **Telegram**: bot info, updates, send
- **Todoist**: tasks, create, close, delete
- **GitLab**: projects, issues, merge requests, pipelines, create issue
- **Home Assistant**: entity states, config, call services
- **Trello**: boards, lists, cards, create card, comment
- **Gmail**: search, read, send
- **Google Calendar**: events, create, delete
- **Google Drive**: search, read/export, metadata, create, delete
- **Google Docs / Sheets**: read, create, append, update ranges
- **GitHub**: repos, branches, issues, PRs (create/merge), Actions runs, re-run failed jobs
- **Outlook**: mail, send, calendar events
- **Spotify**: now playing, play/pause/skip

## Credential overrides

Settings → Connectors lists every service. Paste your own credentials there and Argus stores them in the OS keyring (`argus-connector`); they override any built-in or environment defaults. Leave a field empty to keep what is already saved, and Remove wipes all keyring entries for that service.

- Token services (Linear, Slack, Notion, Figma, Discord, Telegram, Todoist, GitLab, Home Assistant): paste the token — the same one the setup steps describe below.
- Trello: paste the API key and token together.
- OAuth services (Google, GitHub, Outlook, Spotify): paste your own OAuth client ID (and client secret for Google/GitHub), then connect as usual — the override wins over env vars and config files.

## Google

Gmail, Calendar, Drive, Docs, Sheets. Needs a Google Cloud **Desktop-type** OAuth client (loopback redirect, no secret required): enable the five APIs, copy the client id into setup. The agent lists/sends mail, manages events, reads/writes files, docs, and sheets.

## GitHub

Repos, issues, PRs, Actions. Register an OAuth App and connect via device code. The agent browses code, files issues, opens and merges PRs, and re-runs failed jobs.

## Slack

Channels, threads, DMs, users. Create a Slack app, install it to the workspace, paste the bot token (`xoxb-…`). Invite the bot to any private channel you want it to read.

## Discord

Guilds, channels, history, send. Paste the bot token. Flip on the **Message Content intent** in the bot portal or message bodies arrive empty.

## Telegram

DMs and channels via BotFather token. The fastest setup on this page — two minutes.

## Notion

Search, databases, pages, blocks. Create an internal integration, paste the token, and **share each page/database with the integration** or queries return "not found".

## Linear

Issues, cycles, comments over GraphQL. Paste a personal API key. Pick the team once; the agent handles the rest.

## Todoist

Tasks: list, filter, create, close, delete. Paste the token.

## Trello

Boards, lists, cards, comments. Needs an API key (Power-Up admin page) plus a token.

## GitLab

Projects, issues, merge requests, pipelines. Paste a personal access token; self-hosted instances set `base_url`.

## Figma

File structure, comments, post comments. Paste a personal access token.

## Outlook

Mail and calendar via device-code flow (same UX as GitHub). Register the app once in Microsoft Entra, then connect.

## Home Assistant

Entity states, services, config. Set `base_url` (e.g. `http://homeassistant.local:8123`) and paste a long-lived access token.

## Spotify

Now-playing and playback control. Register an app at developer.spotify.com with a `127.0.0.1` redirect, then connect via loopback OAuth.
