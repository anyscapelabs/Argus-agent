# Learning — Feedback Loop (behavioral preferences, not facts)

Code: `src-tauri/src/learning/{mod.rs,store.rs,schema.rs}`, `src/lib/ipc.ts` (learning* bindings), `src/components/{AgentBubble,ChatDetailPage}.tsx` (edit affordance).

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
- User control: `learning_list`, `learning_forget` commands (no Settings UI yet); corrections capped at 16k chars; events/examples content capped.

## Rules for agents

- Keep facts in `memory/`; keep behavior in `learning/`. Never auto-learn from one-off requests, jokes, or task-specific instructions.
- The extractor must stay a filter, not a decider — validation thresholds live in `learning/mod.rs`, not the prompt text.
- Tests: `src-tauri/tests/learning_test.rs` (confidence math, gating, caps, queue, vote/correction validation).
