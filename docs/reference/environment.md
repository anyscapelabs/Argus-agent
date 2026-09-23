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

## OS requirements for the sandbox

No container runtime is needed. Isolation uses what the OS already enforces.

| Platform | Mechanism | Requires |
| --- | --- | --- |
| Linux | Landlock LSM + seccomp + rlimits | kernel 5.13 for filesystem rules; 6.7 for network rules (ABI v4) |
| macOS | `sandbox-exec` (`/usr/bin/sandbox-exec`) | a system where Apple still ships it |
| Windows | Job Objects | — |

Check Landlock on Linux:

```bash
cat /sys/kernel/security/lsm | grep -o landlock   # filesystem isolation
uname -r                                        # 6.7+ for network rules
```

Below these floors an isolated profile is refused, not downgraded to the host.
The default `Host` profile is unaffected.

