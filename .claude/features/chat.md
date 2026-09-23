# Chat — Sessions, Approvals, Streaming

Code: `src-tauri/src/sessions/{chat.rs,mod.rs,store.rs,schema.rs,browser_import.rs,ext_install.rs}`, `src/stores/sessions.ts`, `src/lib/ipc.ts:152-287`, `src/components/{ChatDetailPage,ChatInput,AgentBubble,UserBubble,SessionList,NewAgentPage}.tsx`, `src/components/agent/*.tsx`.

## Loop (`sessions/chat.rs::send`, ≤12 steps)

- Project prompt → stream reply → parse `<action>` → approval gate → `tools::exec` → `<tool-result>` back in. Loop-guard trips on 3rd identical repeat.
- Screenshots by path attach as images (last 2 msgs). Overflow → `prompt::compressor` summarization with utility model.
- `ask_approval()` + `sess_chat_stream(sessionId,content,on_event:Channel<StreamEvent>)`, `sess_cancel_chat`, `sess_resolve_approval(id, allow, args?)` (oneshot `HashMap<String,Sender<ApprovalReply>>` in `Gateway.approvals`; `allow` + optional edited args JSON, validated by `gateway::approval_reply`, applied to `exec.args` before run — browser tools excluded).
- Headless runs: `ChatSink` trait (`Channel` impl forwards, `NullSink` drops; `term_chan()` gates terminal streaming) — `send()` takes `&dyn ChatSink` + a `role` tag (`"user"` default; scheduler turns use `"system"`/`"cron"`), and every finished turn emits `argus://session-activity {session_id, kind:"turn-done"}`. Tests: `chat_headless_test.rs`.
- Skill reflection runs out-of-band: after a 6+ action turn finishes, `run_skill_reflection` fires silently via a null channel and only executes `skill.create` actions — it never persists a visible nudge turn, so the model's real final answer stays last.
- `notice` stream events render as `<warning>` banners in the live turn (previously dropped); a `browser.*` failure matching "Chrome is not connected to Argus" posts the one-time Load-unpacked setup steps instead of reading as a generic failure.

## Schema / commands (`sessions/schema.rs`, `mod.rs`)

- `folders`, `sessions(permission DEFAULT 'ask')`, `messages`, `summaries`. KV prefs: `newagent.permission/model/webSearch`.
- Commands: `sess_create_session, sess_list_sessions, sess_save_session, sess_set_permission, sess_set_model, sess_set_web_search, sess_delete_session, sess_export_json, sess_list_messages, sess_set_vote, sess_clean_dangling, sess_add_message, sess_supersede_from, sess_create_folder, sess_list_folders, newagent_prefs/set_newagent_prefs`.

## Frontend (`src/stores/sessions.ts`)

- `Channel<StreamEvent>`: `delta|reset|step|term|term_end|approval|notice|err`. Optimistic `pending-` user msg, auto-title 60 chars, retry via `sessSupersedeFrom`, `stop()` via `sessCancelChat`.
- Turn structure is backend-driven: the loop-final assistant row is marked `kind='final'` (`store::mark_final`); `ChatDetailPage` renders that row whole and collapses all other assistant turns into compact `ToolActivity` step rows (no prose, no outputs) — positional order is only the fallback for pre-`kind` legacy rows. Prompt rule: status line + actions while working, complete answer only in action-free turns.
- One work panel per turn: `ToolActivity` owns the header (`Working… Ns` ticking while live via `useWorkTimer`, `Worked for Ns` when done), the bordered step container, and thought/plan/tool/terminal/browser rows (thought/plan expandable, terminal has Show-details). `WorkSummary.tsx` deleted. `ThinkingBlock/PlanBlock` remain only for tool-less turns.
- Tool blocks inside the final message itself are folded into the work steps and hidden inline (`AgentBubble hideToolActivity`), so commands never render outside the panel. `<check status=pass|retry>` records render as "Checked the answer" / "Caught an issue, retrying" rows the same way.
- Block ownership is centralized: `blockRole()` in `lib/agentXml.ts` (`work` = action/terminal/sandbox/browser-action/check/document/plan/thinking/step, everything else `content`). The work panel owns `work` blocks whenever it exists; bubbles render content plus `work` only as fallback. `renderTree` enforces this at the top of dispatch — no per-branch hide flags, so `WebSearchGroup` can't leak a second panel.
- Model `<ul>/<li>` lists normalize to `- ` bullets in the prose pass (list structure was silently lost before).
- Email addresses are shielded from tool-mention chips in `AgentBubble::flush` (capture-group `split` on an email regex; emails pass through plain, bare `@tool` mentions still chip) — otherwise `@gmail` inside `user@gmail.com` shreds the address.
- Summary bubbles show prior rows' prose paragraphs/headings plus the final message, so turns ending in a stub still read complete. Backend forces the issue too: faux `<sandbox>` blocks join the done-guard, exhaustion triggers one plain-text summary retry (`SUMMARY_DEMAND`), and the final fallback persists a `<warning>` into the message so the failure stays visible in history.
- Reflection self-check (off by default via `sessions.reflect`, flippable with `sess_set_reflect`): after a loop-final answer with prior work, one utility-model call checks goal-satisfaction; PASS appends a check row and ends the turn, FAIL injects the instruction as a nudge (max 1 retry), check-model errors finalize silently. Pure helpers (`should_reflect`, `parse_reflection_verdict`, `check_block`) tested in `tests/reflect_test.rs`.
- `document` record blocks collected across the turn render as full `DocumentCard`s after the final bubble — creation stays a compact step row, presentation is the card.
- Native (non-XML) tool calls emit a synthetic `<action>` delta when execution starts, so live activity + approval matching work for them too (persisted records still come from the normal append path; the synthetic text is live-only).
- Native calls without their own record block (anything but terminal/browser/doc.create) get a persisted `<action tool=args>` append, so work steps, approvals and history show them exactly like XML-origin calls.
- `Turn{text,term:Record<idx,chunk>,termCode,approval,err}` mutated incrementally; `step` clears turn + reloads; `done` clears + reloads; `err` preserves retryable turn. Background turns fire `notifyDone` only when not watching (`App.tsx:45-70`).
- Rendering: `AgentBubble` parses `lib/agentXml.ts` tags; `ApprovalBlock` Run/Deny; `SessionList` + `listen("sessions-changed")` in `App.tsx:75`.
- Email drafts: `gmail.send`/`outlook.send` pending approvals render as interactive `EmailDraftCard` (editable To/Subject/Body) inside `ToolActivity` via `actionStep` email branch; Send → `resolveApproval(id, true, editedArgs)`, Discard → deny. History `<email-draft>` blocks render read-only.

## Rules for agents

- Preserve `ask` default. Denials final, no auto-retry. `never` only for explicit trusted flows.
- Keep `ipc.ts` ↔ `lib.rs` names in sync (`sess_*` snake + camel args).
- Tests: `src-tauri/tests/chat_test.rs, chat_exec_test.rs, chat_media_test.rs, sessions_test.rs`.
