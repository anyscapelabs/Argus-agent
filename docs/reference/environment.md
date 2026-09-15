# Environment reference
> **Job:** Every `ARGUS_*` variable in one place.

| Variable | Purpose |
| --- | --- |
| `ARGUS_GOOGLE_CLIENT_ID` / `ARGUS_GOOGLE_CLIENT_SECRET` | Google OAuth client (overrides vault/files) |
| `ARGUS_GITHUB_CLIENT_ID` / `ARGUS_GITHUB_CLIENT_SECRET` | GitHub OAuth app |
| `ARGUS_OUTLOOK_CLIENT_ID` | Microsoft Entra app |
| `ARGUS_SPOTIFY_CLIENT_ID` | Spotify dev app |
| `ARGUS_GITLAB_URL` | Self-hosted GitLab base URL |
| `ARGUS_HA_URL` | Home Assistant base URL |
| `ARGUS_TRELLO_KEY` | Trello API key |
| `ARGUS_CHROME` | Chrome binary override for browser tools |
| `XDG_DATA_HOME` | App data root override (default `~/.local/share`) |

Client IDs and tokens are better kept in the OS keyring via Connectors setup; env vars win when both exist.
