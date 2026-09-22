# Prompt — System Assembly + Compaction

Code: `src-tauri/src/prompt/{mod.rs,config.rs,compressor.rs}`.

## Assembly (`prompt::project`)

Canonical brainstem (`BASE`: identity, core rules, terminal, tool selection, skills/memory retrieval, recovery, permissions, response + minimal response-format block) + tool section (`tools::section`: action protocol, guidance) + optional `<user-preferences>` + optional `<session-summary>` + optional `<working-notes>` (1024-byte cap). No skill index, no automatic memory injection, no session-start recall — skills/memories load on demand via their tools; `bash.run` hidden from the model (runtime alias kept). Budgets via `prompt::budget` + `prompt::tools_budget` + `full_budget()` PromptBudget struct (skill/memory tiers always 0).

## Config / compaction

- `config.rs`: per-model context windows. `compressor.rs`: overflow summarization with utility (cheapest enabled) model. Titles also run on cheapest model.
- Commands: `prompt_preview, prompt_status, prompt_compact`.

## Rules for agents

- Keep prompt deterministic and secret-free (no keys/tokens in assembled prompt).
- Tool spec changes in `tools/mod.rs::tool_specs/protocol_section/guidance` must be reflected in `prompt_preview` snapshot tests.
- Tests: `src-tauri/tests/prompts_test.rs` (brainstem contract), `prompts_budget_test.rs` (no skill index/memory dump/bash.run; terminal+admin+password rules; optional summary/prefs; budget sanity).
