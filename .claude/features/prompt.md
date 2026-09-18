# Prompt — System Assembly + Compaction

Code: `src-tauri/src/prompt/{mod.rs,config.rs,compressor.rs}`.

## Assembly (`prompt::project`)

Base identity + reply-format rules + tool section + user preferences + skill index (name + 1-line) + latest compaction summary.

## Config / compaction

- `config.rs`: per-model context windows. `compressor.rs`: overflow summarization with utility (cheapest enabled) model. Titles also run on cheapest model.
- Commands: `prompt_preview, prompt_status, prompt_compact`.

## Rules for agents

- Keep prompt deterministic and secret-free (no keys/tokens in assembled prompt).
- Tool spec changes in `tools/mod.rs::tool_specs/protocol_section/guidance` must be reflected in `prompt_preview` snapshot tests.
- Tests: `src-tauri/tests/prompts_test.rs`.
