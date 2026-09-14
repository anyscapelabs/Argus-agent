import { Channel, invoke } from "@tauri-apps/api/core";

const CMD_GW_LIST_PROV = "gw_list_providers";
const CMD_GW_PROV_MODS = "gw_provider_models";
const CMD_GW_CHAT_MODS = "gw_chat_models";
const CMD_GW_SET_MOD = "gw_set_model_enabled";
const CMD_GW_CONN = "gw_connect";
const CMD_GW_DISC = "gw_disconnect";
const CMD_GW_SYNC = "gw_sync_providers";
const CMD_GW_LOGO = "gw_logo";
const CMD_GW_ROUTE = "gw_set_routing";
const CMD_SESS_CREATE = "sess_create_session";
const CMD_SESS_LIST = "sess_list_sessions";
const CMD_SESS_DEL = "sess_delete_session";
const CMD_SESS_SAVE = "sess_save_session";
const CMD_SESS_PERM = "sess_set_permission";
const CMD_SESS_MOD = "sess_set_model";
const CMD_SESS_WEB = "sess_set_web_search";
const CMD_SESS_EXP = "sess_export_json";
const CMD_SESS_MSGS = "sess_list_messages";
const CMD_SESS_VOTE = "sess_set_vote";
const CMD_SESS_SUP = "sess_supersede_from";
const CMD_SESS_CLEAN = "sess_clean_dangling";
const CMD_SESS_APPR = "sess_resolve_approval";
const CMD_SESS_IMPORT = "sess_browser_import";
const CMD_SESS_EXT_IN = "sess_ext_install";
const CMD_SESS_EXT_UN = "sess_ext_uninstall";
const CMD_SESS_EXT_ST = "sess_ext_status";
const CMD_SESS_STREAM = "sess_chat_stream";
const CMD_GOOGLE_STATUS = "google_status";
const CMD_GOOGLE_CONN_URL = "google_connect_url";
const CMD_GOOGLE_DISC = "google_disconnect";
const CMD_GITHUB_STATUS = "github_status";
const CMD_GITHUB_CONN = "github_connect";
const CMD_GITHUB_DISC = "github_disconnect";

const DEF_PERM = "ask";

export class IpcError extends Error {}
export class SessIpcError extends IpcError {}
export class GwIpcError extends IpcError {}

export type Provider = {
  id: string;
  name: string;
  compatible: string;
  baseUrl: string;
  apiKeyRef: string | null;
  connected: boolean;
  free: boolean;
  priority: number;
  logoUrl: string | null;
  docUrl: string | null;
};

export type SyncStats = {
  providers: number;
  models: number;
};

export type ProviderModel = {
  providerId: string;
  modelId: string;
  displayName: string;
  capabilities: string | null;
  enabled: boolean;
  costIn: number;
  costOut: number;
};

export type ChatModel = {
  modelId: string;
  displayName: string;
  providerId: string;
  providerName: string;
};

export function gwListProviders(): Promise<Provider[]> {
  return invoke<Provider[]>(CMD_GW_LIST_PROV);
}

export function gwProviderModels(): Promise<ProviderModel[]> {
  return invoke<ProviderModel[]>(CMD_GW_PROV_MODS);
}

export function gwChatModels(): Promise<ChatModel[]> {
  return invoke<ChatModel[]>(CMD_GW_CHAT_MODS);
}

export function gwSetModelEnabled(
  modelId: string,
  enabled: boolean,
): Promise<void> {
  return invoke<void>(CMD_GW_SET_MOD, { modelId, enabled });
}

export function gwConnect(
  providerId: string,
  apiKey?: string,
): Promise<void> {
  return invoke<void>(CMD_GW_CONN, { providerId, tok: apiKey ?? null });
}

export function gwDisconnect(providerId: string): Promise<void> {
  return invoke<void>(CMD_GW_DISC, { providerId });
}

export function gwSyncProviders(): Promise<SyncStats> {
  return invoke<SyncStats>(CMD_GW_SYNC);
}

export function gwLogo(providerId: string): Promise<string | null> {
  return invoke<string | null>(CMD_GW_LOGO, { providerId });
}

export function gwSetRouting(mode: string, pinned: string): Promise<void> {
  return invoke<void>(CMD_GW_ROUTE, { mode, pinned });
}

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
  | { type: "err"; msg: string }
  | { type: "term"; idx: number; chunk: string }
  | { type: "term_end"; idx: number; code: number }
  | { type: "approval"; id: string; idx: number; command: string }
  | { type: "notice"; msg: string };

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
): Promise<void> {
  return invoke<void>(CMD_SESS_APPR, { approvalId, allow });
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

export function sessChatStream(
  sessionId: string,
  content: string,
  onEvent: Channel<StreamEvent>,
): Promise<void> {
  return invoke<void>(CMD_SESS_STREAM, { sessionId, content, onEvent });
}

export function sessCancelChat(sessionId: string): Promise<boolean> {
  return invoke<boolean>("sess_cancel_chat", { sessionId });
}

export type GoogleStatus = {
  connected: boolean;
  email: string | null;
};

export function googleStatus(): Promise<GoogleStatus> {
  return invoke<GoogleStatus>(CMD_GOOGLE_STATUS);
}

export function googleConnectUrl(): Promise<{ url: string }> {
  return invoke<{ url: string }>(CMD_GOOGLE_CONN_URL);
}

export function googleDisconnect(): Promise<void> {
  return invoke<void>(CMD_GOOGLE_DISC);
}

export type GithubStatus = {
  connected: boolean;
  login: string | null;
};

export type GithubDevice = {
  verificationUri: string;
  userCode: string;
};

export function githubStatus(): Promise<GithubStatus> {
  return invoke<GithubStatus>(CMD_GITHUB_STATUS);
}

export function githubConnect(): Promise<GithubDevice> {
  return invoke<GithubDevice>(CMD_GITHUB_CONN);
}

export function githubDisconnect(): Promise<void> {
  return invoke<void>(CMD_GITHUB_DISC);
}

export type Skill = {
  name: string;
  description: string;
  body: string;
  source: string;
  origin: string | null;
  use_count: number;
  last_used_at: string | null;
  created_at: string;
};

export function skillList(): Promise<Skill[]> {
  return invoke<Skill[]>("skill_list");
}

export function skillSearch(query: string): Promise<Skill[]> {
  return invoke<Skill[]>("skill_search", { query });
}

export function skillGet(name: string): Promise<Skill> {
  return invoke<Skill>("skill_get", { name });
}

export function skillDelete(name: string): Promise<void> {
  return invoke<void>("skill_delete", { name });
}

export type Memory = {
  id: string;
  content: string;
  kind: string;
  importance: number;
  session_id: string | null;
  created_at: string;
  updated_at: string;
};

export type MemoryNode = {
  id: string;
  label: string;
  kind: string;
};

export type MemoryEdge = {
  from_id: string;
  to_id: string;
  relation: string;
};

export type MemoryGraph = {
  nodes: MemoryNode[];
  edges: MemoryEdge[];
};

export type RecallHit = {
  source: string;
  ref_id: string;
  session_id: string | null;
  snippet: string;
};

export function memoryList(kind?: string): Promise<Memory[]> {
  return invoke<Memory[]>("memory_list", { kind: kind ?? null });
}

export function memorySearch(query: string): Promise<Memory[]> {
  return invoke<Memory[]>("memory_search", { query });
}

export function memoryRecall(query: string): Promise<RecallHit[]> {
  return invoke<RecallHit[]>("memory_recall", { query });
}

export function memoryGraph(): Promise<MemoryGraph> {
  return invoke<MemoryGraph>("memory_graph", {});
}

export function memoryDelete(id: string): Promise<void> {
  return invoke<void>("memory_delete", { id });
}

export type NewAgentPrefs = {
  modelId: string | null;
  permission: string;
  webSearch: boolean;
};

export function newagentPrefs(): Promise<NewAgentPrefs> {
  return invoke<NewAgentPrefs>("newagent_prefs");
}

export function setNewagentPrefs(prefs: {
  modelId: string | null;
  permission: string;
  webSearch: boolean;
}): Promise<void> {
  return invoke<void>("set_newagent_prefs", prefs);
}
