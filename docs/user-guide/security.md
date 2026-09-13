# Security
> **Job:** What keeps the agent from doing damage, and where secrets live.

## Approvals

`ask` mode pauses mutating tools (terminal writes, file writes, desktop actions, browser logins/checkouts) for an explicit Run/Deny. Denials are final for that action.

## Secrets

| Secret | Storage |
| --- | --- |
| Provider API keys | OS keyring (`argus-gw`) |
| Connector tokens | OS keyring (`argus-connector`, `argus-google`, `argus-github`) |
| OAuth client IDs | Keyring first, env vars, then local settings files |

No key or token is ever written to chat history, logs, or the repo. URLs carrying credentials are refused before they load, and credential shapes in tool output are redacted. Connector log rows redact `token=`/`key=` values.

## Untrusted content

Web pages and screen text are data, never instructions. The agent reports prompt-injection attempts instead of obeying them, and never types passwords or payment details.
