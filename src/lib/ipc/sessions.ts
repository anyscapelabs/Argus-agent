// Sessions, their messages, and the tool events the UI replays.
import { invoke } from "@tauri-apps/api/core";

import {
  CMD_SESS_APPR,
  CMD_SESS_CLEAN,
  CMD_SESS_CREATE,
  CMD_SESS_DEL,
  CMD_SESS_EVENTS,
  CMD_SESS_EXP,
  CMD_SESS_EXT_IN,
  CMD_SESS_EXT_ST,
  CMD_SESS_EXT_UN,
  CMD_SESS_IMPORT,
  CMD_SESS_LIST,
  CMD_SESS_LOCAL,
  CMD_SESS_MOD,
  CMD_SESS_MSGS,
  CMD_SESS_PERM,
  DEF_PERM,
  CMD_SESS_SAVE,
  CMD_SESS_SUP,
  CMD_SESS_VOTE,
  CMD_SESS_WEB,
} from "./commands";


export type SessionRow = {
  id: string;
  title: string;
  status: string;
  model_id: string | null;
  permission: string;
  folder_id: string | null;
  created_at: string;
  updated_at: string;
  ctx_tokens: number;
  compact_seq: number;
  compactions: number;
  web_search: boolean;
  profile_id: string | null;
  running_agents: number;
};

export type MsgRow = {
  id: string;
  session_id: string;
  seq: number;
  role: string;
  content: string;
  model_id: string | null;
  provider_id: string | null;
  tok_in: number | null;
  tok_out: number | null;
  active: boolean;
  vote: string | null;
  tool_calls?: string | null;
  tool_call_id?: string | null;
  kind?: string | null;
  attachments?: string | null;
  /// The app answered this line itself, so it is a turn the transcript draws
  /// and a line the model is never shown.
  local?: boolean;
  created_at: string;
};

export type StreamEvent =
  | { type: "status"; provider_id: string; attempt: number }
  | { type: "delta"; text: string }
  | { type: "reset" }
  | { type: "step" }
  | {
      type: "done";
      model_id: string;
      provider_id: string;
      attempt: number;
      latency_ms: number;
      tok_in: number;
      tok_out: number;
      cost: number;
    }
  | { type: "refresh" }
  | { type: "err"; msg: string; kind: string }
  | {
      type: "retry";
      attempt: number;
      max_attempts: number;
      wait_secs: number;
      label: string;
    }
  | { type: "term"; idx: number; chunk: string }
  | { type: "term_end"; idx: number; code: number }
  | { type: "approval"; id: string; idx: number; command: string }
  | { type: "notice"; msg: string }
  | { type: "turn_end"; session_id: string };

export function sessCreateSession(
  title: string,
  modelId: string | null,
  permission: string = DEF_PERM,
  webSearch: boolean = false,
): Promise<SessionRow> {
  return invoke<SessionRow>(CMD_SESS_CREATE, {
    req: {
      title,
      model_id: modelId,
      permission,
      folder_id: null,
      web_search: webSearch,
    },
  });
}

export function sessListSessions(): Promise<SessionRow[]> {
  return invoke<SessionRow[]>(CMD_SESS_LIST);
}

export function sessDeleteSession(sessionId: string): Promise<void> {
  return invoke<void>(CMD_SESS_DEL, { sessionId });
}

export function sessSaveSession(session: SessionRow): Promise<void> {
  return invoke<void>(CMD_SESS_SAVE, { session });
}

export function sessSetPermission(
  sessionId: string,
  permission: string,
): Promise<void> {
  return invoke<void>(CMD_SESS_PERM, { sessionId, permission });
}

export function sessSetModel(
  sessionId: string,
  modelId: string | null,
): Promise<void> {
  return invoke<void>(CMD_SESS_MOD, { sessionId, modelId });
}

export function sessSetWebSearch(
  sessionId: string,
  on: boolean,
): Promise<void> {
  return invoke<void>(CMD_SESS_WEB, { sessionId, on });
}

export function sessExportJson(sessionId: string): Promise<string> {
  return invoke<string>(CMD_SESS_EXP, { sessionId });
}

export function sessListMessages(sessionId: string): Promise<MsgRow[]> {
  return invoke<MsgRow[]>(CMD_SESS_MSGS, { sessionId });
}

export type ToolEvent = {
  id: string;
  message_id: string;
  session_id: string;
  kind: string;
  tool: string;
  args_json: string;
  status: string;
  elapsed_ms: number;
  code: number;
  output: string;
  label: string;
  detail: string;
  created_at: string;
};

export function sessListEvents(sessionId: string): Promise<ToolEvent[]> {
  return invoke<ToolEvent[]>(CMD_SESS_EVENTS, { sessionId });
}

/// Store a line the app answers itself as a turn. The backend refuses a command
/// it does not answer that way, so this is only ever called for `/usage`.
export function sessRunLocal(
  sessionId: string,
  name: string,
  arg: string,
): Promise<MsgRow> {
  return invoke<MsgRow>(CMD_SESS_LOCAL, { sessionId, name, arg });
}

export function sessSetVote(
  sessionId: string,
  msgId: string,
  vote: "up" | "down" | null,
): Promise<void> {
  return invoke<void>(CMD_SESS_VOTE, { sessionId, msgId, vote });
}

export function sessSupersedeFrom(
  sessionId: string,
  seq: number,
): Promise<void> {
  return invoke<void>(CMD_SESS_SUP, { sessionId, seq });
}

export function sessCleanDangling(sessionId: string): Promise<number> {
  return invoke<number>(CMD_SESS_CLEAN, { sessionId });
}

export function sessResolveApproval(
  approvalId: string,
  allow: boolean,
  args?: string,
): Promise<void> {
  return invoke<void>(CMD_SESS_APPR, { approvalId, allow, args: args ?? null });
}

export function sessBrowserImport(profile: string): Promise<void> {
  return invoke<void>(CMD_SESS_IMPORT, { profile });
}

export type ExtInstall = {
  extId: string;
  extPath: string;
};

export function sessExtInstall(): Promise<ExtInstall> {
  return invoke<ExtInstall>(CMD_SESS_EXT_IN);
}

export function sessExtUninstall(): Promise<void> {
  return invoke<void>(CMD_SESS_EXT_UN);
}

export function sessExtStatus(): Promise<boolean> {
  return invoke<boolean>(CMD_SESS_EXT_ST);
}
