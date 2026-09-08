import { invoke } from "@tauri-apps/api/core";

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

// gateway
export const gwListProviders = () => invoke<Provider[]>("gw_list_providers");

export const gwProviderModels = () => invoke<ProviderModel[]>("gw_provider_models");

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
