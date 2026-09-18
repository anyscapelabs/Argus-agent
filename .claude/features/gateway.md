# Gateway — Providers, Models, Router

Code: `src-tauri/src/gateway/{mod.rs,store.rs,catalog.rs,router.rs,schema.rs,adapters/{mod.rs,anthropic.rs,openai_compat.rs}}`, `src/hooks/{useProviders,useChatModels,useModels,useProviderLogo}.ts`, `src/lib/ipc.ts:43-118`, `src/components/settings/*`.

## Structure

- `Gateway{conn:Mutex<Connection>, http, skills_dir, library_dir, logos_dir, approvals, tasks}` (`gateway/mod.rs`).
- Commands: `gw_list_providers, gw_upsert_provider, gw_list_models, gw_provider_models, gw_chat_models, gw_set_model_enabled, gw_add_model, gw_link_model, gw_connect, gw_disconnect, gw_set_routing, gw_chat, gw_chat_stream(Channel<StreamEvent>), gw_sync_providers, gw_logo, gw_logs`.
- `store.rs`: rusqlite + `keyring::Entry::new("argus-gw", provider_id)` get/set/delete. Secrets never in DB.
- `adapters/mod.rs`: `dispatch|dispatch_stream` by `compatible: Anthropic|OpenAI`, `verify_key()` auth-probe, `openai_msgs(), anthropic_content(), sse_events(), wire_name/real_name (dot→underscore), shot_bytes()` only `/screenshots/shot-*.png`, `is_local()`.
- `catalog.rs`: `maybe_sync_catalog()` spawned in `lib.rs:80` + models.dev sync.
- `router.rs:269 stream_run(chan)`: `prefer_free|prefer_paid|pinned` failover, retryable `402/408/429/5xx`, costs from usage.

## Dialects (README)

- Anthropic API + OpenAI-compatible (OpenAI, Baseten, self-hosted Ollama, any OpenAI-style endpoint).
- Setup flow: Settings → Providers → paste key → connect → enable models in Settings → Models. Keys in OS keyring.

## Frontend

- `ipc.ts` types `Provider, ProviderModel, ChatModel, SyncStats` + `gwListProviders/ProviderModels/ChatModels/SetModelEnabled/Connect/Disconnect/SyncProviders/Logo/SetRouting`.
- `useProviderLogo.ts` caches `data:image/svg+xml`. Settings pages: `ProvidersPage, ModelsPage, ProviderConnectModal, ConnectedProviderList, SettingsSidebar` + `SettingsModal.tsx`.

## Rules for agents

- New provider dialect → new adapter in `adapters/` + `dispatch` branch + `verify_key` probe + `gw_*` commands in `lib.rs` + `ipc.ts` binding.
- Never log keys. `gw_logs` must redact. Tests: `src-tauri/tests/adapters_test.rs, mcp_test.rs`.
