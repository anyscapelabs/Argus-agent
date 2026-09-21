# Chat — Sessions, Approvals, Streaming

Code: `src-tauri/src/sessions/{chat.rs,mod.rs,store.rs,schema.rs,browser_import.rs,ext_install.rs}`, `src/stores/sessions.ts`, `src/lib/ipc.ts:152-287`, `src/components/{ChatDetailPage,ChatInput,AgentBubble,UserBubble,SessionList,NewAgentPage}.tsx`, `src/components/agent/*.tsx`.

## Loop (`sessions/chat.rs::send`, ≤12 steps)

- Project prompt → stream reply → parse `<action>` → approval gate → `tools::exec` → `<tool-result>` back in. Loop-guard trips on 3rd identical repeat.
- Screenshots by path attach as images (last 2 msgs). Overflow → `prompt::compressor` summarization with utility model.
- `ask_approval()` + `sess_chat_stream(sessionId,content,on_event:Channel<StreamEvent>)`, `sess_cancel_chat`, `sess_resolve_approval(id, allow, args?)` (oneshot `HashMap<String,Sender<ApprovalReply>>` in `Gateway.approvals`; `allow` + optional edited args JSON, validated by `gateway::approval_reply`, applied to `exec.args` before run — browser tools excluded).

## Schema / commands (`sessions/schema.rs`, `mod.rs`)

- `folders`, `sessions(permission DEFAULT 'ask')`, `messages`, `summaries`. KV prefs: `newagent.permission/model/webSearch`.
- Commands: `sess_create_session, sess_list_sessions, sess_save_session, sess_set_permission, sess_set_model, sess_set_web_search, sess_delete_session, sess_export_json, sess_list_messages, sess_set_vote, sess_clean_dangling, sess_add_message, sess_supersede_from, sess_create_folder, sess_list_folders, newagent_prefs/set_newagent_prefs`.

## Frontend (`src/stores/sessions.ts`)

- `Channel<StreamEvent>`: `delta|reset|step|term|term_end|approval|notice|err`. Optimistic `pending-` user msg, auto-title 60 chars, retry via `sessSupersedeFrom`, `stop()` via `sessCancelChat`.
- `Turn{text,term:Record<idx,chunk>,termCode,approval,err}` mutated incrementally; `step` clears turn + reloads; `done` clears + reloads; `err` preserves retryable turn. Background turns fire `notifyDone` only when not watching (`App.tsx:45-70`).
- Rendering: `AgentBubble` parses `lib/agentXml.ts` tags; `ApprovalBlock` Run/Deny; `SessionList` + `listen("sessions-changed")` in `App.tsx:75`.
- Email drafts: `gmail.send`/`outlook.send` pending approvals render as interactive `EmailDraftCard` (editable To/Subject/Body) inside `ToolActivity` via `actionStep` email branch; Send → `resolveApproval(id, true, editedArgs)`, Discard → deny. History `<email-draft>` blocks render read-only.

## Rules for agents

- Preserve `ask` default. Denials final, no auto-retry. `never` only for explicit trusted flows.
- Keep `ipc.ts` ↔ `lib.rs` names in sync (`sess_*` snake + camel args).
- Tests: `src-tauri/tests/chat_test.rs, chat_exec_test.rs, chat_media_test.rs, sessions_test.rs`.
