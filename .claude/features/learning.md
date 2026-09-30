# Learning — Feedback Loop (behavioral preferences, not facts)

Code: `src-tauri/src/learning/{mod.rs,store.rs,schema.rs}`, `src/lib/ipc/` (learning* bindings), `src/components/{AgentBubble,ChatDetailPage}.tsx` (edit affordance).

## Model

Learning stores **how the user wants Argus to behave** (style, formatting, workflows); `memory/` stores facts about the user/world. Never learn durable facts here — those stay in memory.

## Tables (`learning/schema.rs` MIGRATE)

- `learning_events(id, session_id, message_id, kind, payload, processed, created_at, processed_at)` — raw signals: `vote`, `correction`, `explicit_feedback`.
- `learning_preferences(id, scope, category, key, value, confidence, explicit, evidence_count, last_confirmed, created_at, updated_at)` with `UNIQUE(scope, category, key)` — confidence-gated behavioral prefs.
- `learning_examples(id, scope, kind, content, quality, source, created_at)` — writing samples, capped at the 12 strongest per scope+kind.

## Flow

- Signals: `sess_set_vote` → `learning::record_vote` (votes alone never become prefs); `AgentBubble` pencil → `learningRecordCorrection` → `learning::record_correction` (original pulled from DB, never trusted from the client); `learning_feedback` for free-text feedback.
- Background: `learn_pending` fires after each finished turn (spawned in `chat.rs` like skill reflection). `extract_candidate` runs the utility model as a filter (single votes → `learn=false`; corrections diff ORIGINAL vs EDITED); `process_one` applies only candidates with confidence ≥ 0.72, explicit prefs never decay, inferred blend 0.65/0.35, then marks the event processed. Failures leave events pending for retry.
- Prompt: `learning::prompt_context` injects a bounded `<learned-preferences>` block (confidence ≥ 0.62, ≤12 prefs, ≤2 writing examples) between stable and summary in `prompt::project`. Budgeted as the `learned` tier in `PromptBudget`.
- User control: `learning_list`, `learning_forget` commands (no Settings UI yet); corrections capped at 16k chars; events/examples content capped. Neither command touches `playbook_items` — `store::forget` / `store::forget_all` are the only way out, deliberately not yet surfaced.
- `playbook/` is the protocol ledger, separate from the preference ledger on purpose: a preference is how the user wants Argus to behave, a playbook entry is what the harness *observed*. `protocol_events` (raw, capped per kind+scope, deduped by bumping `seen`) feed `playbook_items` (one fixed sentence per kind, keyed for evidence). Nothing is model-generated — sentences live in `playbook/mod.rs` so a model cannot rewrite the rules it is judged by. Scopes are load-bearing: `model` for how a model speaks, `host` for what this machine can enforce (`ext_install::host_id()`), so a sandbox refusal is never filed as model misbehaviour. Signals match strings Argus wrote about itself (`EMPTY_ARGS_ERR`, `MISSING_CWD_ERR`, `sandbox::DENIAL_NOTE`), never model prose. `curate` runs after a finished turn, evidence is *set* not incremented (`MIN_EVIDENCE_TO_TEACH = 2`), and a sub-agent never inherits the parent's playbook. Tests: `playbook_test.rs` (store), `playbook_signals_test.rs` (forgery), `playbook_prompt_test.rs` (control arm), `playbook_eval_test.rs` (a lesson must change the choice, and no lesson may blame the model).

## Rules for agents

- Keep facts in `memory/`; keep behavior in `learning/`. Never auto-learn from one-off requests, jokes, or task-specific instructions.
- The extractor must stay a filter, not a decider — validation thresholds live in `learning/mod.rs`, not the prompt text.
- Tests: `src-tauri/tests/learning_test.rs` (confidence math, gating, caps, queue, vote/correction validation).
