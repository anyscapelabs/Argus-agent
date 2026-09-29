// The fourteen connectors: their OAuth status, the shared credential store,
// and the tool catalog the backend publishes.
import { invoke } from "@tauri-apps/api/core";

import {
  CMD_CONN_CATALOG,
  CMD_CONN_CLEAR_CLI,
  CMD_CONN_CLEAR_SEC,
  CMD_CONN_HAS_CLI,
  CMD_CONN_HAS_TOK,
  CMD_CONN_REMOVE,
  CMD_CONN_SAVE_CLI,
  CMD_CONN_SAVE_SEC,
  CMD_CONN_SAVE_TOK,
  CMD_GITHUB_CONN,
  CMD_GITHUB_DISC,
  CMD_GITHUB_STATUS,
  CMD_GOOGLE_CONN_URL,
  CMD_GOOGLE_DISC,
  CMD_GOOGLE_STATUS,
  CMD_OUTLOOK_CONN,
  CMD_OUTLOOK_DISC,
  CMD_OUTLOOK_STATUS,
  CMD_SPOT_DISC,
  CMD_SPOT_STATUS,
  CMD_SPOT_URL,
} from "./commands";

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

export type OutlookStatus = {
  connected: boolean;
};

export type OutlookDevice = {
  verificationUri: string;
  userCode: string;
};

export function outlookStatus(): Promise<OutlookStatus> {
  return invoke<OutlookStatus>(CMD_OUTLOOK_STATUS);
}

export function outlookConnect(): Promise<OutlookDevice> {
  return invoke<OutlookDevice>(CMD_OUTLOOK_CONN);
}

export function outlookDisconnect(): Promise<void> {
  return invoke<void>(CMD_OUTLOOK_DISC);
}

export type SpotifyStatus = {
  connected: boolean;
};

export function spotifyStatus(): Promise<SpotifyStatus> {
  return invoke<SpotifyStatus>(CMD_SPOT_STATUS);
}

export function spotifyConnectUrl(): Promise<{ url: string }> {
  return invoke<{ url: string }>(CMD_SPOT_URL);
}

export function spotifyDisconnect(): Promise<void> {
  return invoke<void>(CMD_SPOT_DISC);
}

export function connSaveToken(service: string, token: string): Promise<void> {
  return invoke<void>(CMD_CONN_SAVE_TOK, { service, token });
}

export function connHasToken(service: string): Promise<boolean> {
  return invoke<boolean>(CMD_CONN_HAS_TOK, { service });
}

export function connHasClient(service: string): Promise<boolean> {
  return invoke<boolean>(CMD_CONN_HAS_CLI, { service });
}

export function connRemoveToken(service: string): Promise<void> {
  return invoke<void>(CMD_CONN_REMOVE, { service });
}

export function connSaveClient(
  service: string,
  clientId: string,
): Promise<void> {
  return invoke<void>(CMD_CONN_SAVE_CLI, { service, clientId });
}

export function connSaveSecret(service: string, secret: string): Promise<void> {
  return invoke<void>(CMD_CONN_SAVE_SEC, { service, secret });
}

export function connClearClient(service: string): Promise<void> {
  return invoke<void>(CMD_CONN_CLEAR_CLI, { service });
}

export function connClearSecret(service: string): Promise<void> {
  return invoke<void>(CMD_CONN_CLEAR_SEC, { service });
}

export type CatalogTool = {
  name: string;
  desc: string;
  mutating: boolean;
};

export function connectorTools(): Promise<CatalogTool[]> {
  return invoke<CatalogTool[]>(CMD_CONN_CATALOG);
}
