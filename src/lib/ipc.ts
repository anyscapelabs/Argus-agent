import { Channel, invoke } from "@tauri-apps/api/core";

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

export type SyncStats = { providers: number; models: number };

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

// gateway
export const gwListProviders = () => invoke<Provider[]>("gw_list_providers");

export const gwProviderModels = () => invoke<ProviderModel[]>("gw_provider_models");

export const gwChatModels = () => invoke<ChatModel[]>("gw_chat_models");

export const gwSetModelEnabled = (modelId: string, enabled: boolean) =>
  invoke<void>("gw_set_model_enabled", { modelId, enabled });

export const gwConnect = (providerId: string, tok?: string) =>
  invoke<void>("gw_connect", { providerId, tok: tok ?? null });

export const gwDisconnect = (providerId: string) =>
  invoke<void>("gw_disconnect", { providerId });

export const gwSyncProviders = () => invoke<SyncStats>("gw_sync_providers");

export const gwLogo = (providerId: string) => invoke<string | null>("gw_logo", { providerId });

export const gwSetRouting = (mode: string, pinned: string) =>
  invoke<void>("gw_set_routing", { mode, pinned });

// sessions — wire structs are snake_case (no serde rename on the Rust side)
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
  | { type: "err"; msg: string };

export const sessCreateSession = (title: string, modelId: string | null, permission: string = "ask") =>
  invoke<SessionRow>("sess_create_session", {
    req: { title, model_id: modelId, permission, folder_id: null },
  });

export const sessListSessions = () => invoke<SessionRow[]>("sess_list_sessions");

export const sessDeleteSession = (sessionId: string) =>
  invoke<void>("sess_delete_session", { sessionId });

export const sessSaveSession = (session: SessionRow) =>
  invoke<void>("sess_save_session", { session });

export const sessSetPermission = (sessionId: string, permission: string) =>
  invoke<void>("sess_set_permission", { sessionId, permission });

export const sessSetModel = (sessionId: string, modelId: string | null) =>
  invoke<void>("sess_set_model", { sessionId, modelId });

export const sessExportJson = (sessionId: string) =>
  invoke<string>("sess_export_json", { sessionId });

export const sessListMessages = (sessionId: string) =>
  invoke<MsgRow[]>("sess_list_messages", { sessionId });

export const sessSetVote = (sessionId: string, msgId: string, vote: "up" | "down" | null) =>
  invoke<void>("sess_set_vote", { sessionId, msgId, vote });

export const sessSupersedeFrom = (sessionId: string, seq: number) =>
  invoke<void>("sess_supersede_from", { sessionId, seq });

export const sessCleanDangling = (sessionId: string) =>
  invoke<number>("sess_clean_dangling", { sessionId });

export const sessChatStream = (
  sessionId: string,
  content: string,
  onEvent: Channel<StreamEvent>,
) => invoke<void>("sess_chat_stream", { sessionId, content, onEvent });
