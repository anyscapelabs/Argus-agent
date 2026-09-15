# Providers
> **Job:** Connect model providers and choose models.

## Connecting

Settings → Providers → pick a provider → paste its API key → Connect. Argus verifies the key, then lists the provider's models.

Supported: Anthropic, OpenAI, and any OpenAI-compatible endpoint (custom base URL). Local endpoints that need no key work too.

## Catalog and routing

Settings → Models shows every known model. Toggle models on/off, then **Sync catalog** to pull the latest list from [models.dev](https://models.dev).

Routing modes:

| Mode | Behavior |
| --- | --- |
| `prefer_free` | Free models first. |
| `prefer_paid` | Paid models first. |
| `pinned` | One chosen provider always wins. |

## Sessions pick models

Each session stores a model id, or `Auto` to let routing decide. Costs track per request where the provider reports usage.

## Keys

Provider keys live in the OS keyring under `argus-gw`. They never touch disk in plain text. Disconnecting removes the key.
