// Providers, models, and routing.
import { invoke } from "@tauri-apps/api/core";

import {
  CMD_GW_CHAT_MODS,
  CMD_GW_CONN,
  CMD_GW_DISC,
  CMD_GW_LIST_PROV,
  CMD_GW_LOGO,
  CMD_GW_PROV_MODS,
  CMD_GW_ROUTE,
  CMD_GW_SET_MOD,
  CMD_GW_SYNC,
} from "./commands";


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

export function gwConnect(providerId: string, apiKey?: string): Promise<void> {
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
