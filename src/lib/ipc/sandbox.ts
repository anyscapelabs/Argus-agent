// Sandbox configuration and its self-test.
import { invoke } from "@tauri-apps/api/core";

import {
  CMD_SANDBOX_CFG,
  CMD_SANDBOX_RUNS,
  CMD_SANDBOX_SELFTEST,
  CMD_SANDBOX_SET,
} from "./commands";


export type SandboxProfile = "restricted" | "project" | "host";

export type SandboxConfig = {
  hosts: string[];
  defaultProfile: SandboxProfile;
  netAllow: number[];
};

export type SandboxRun = {
  id: string;
  tool: string;
  command: string;
  profile: string;
  backend: string;
  origin: string | null;
  permission: string;
  startedMs: number;
  durationMs: number;
  exit: number;
  termination: string;
  outBytes: number;
  truncated: boolean;
};

export function sandboxConfig(): Promise<SandboxConfig> {
  return invoke<SandboxConfig>(CMD_SANDBOX_CFG);
}

export function sandboxSetConfig(cfg: SandboxConfig): Promise<void> {
  return invoke<void>(CMD_SANDBOX_SET, { cfg });
}

export function sandboxRuns(limit?: number): Promise<SandboxRun[]> {
  return invoke<SandboxRun[]>(CMD_SANDBOX_RUNS, { limit });
}

export type SandboxProbe = {
  ok: boolean;
  backend: string;
  enforcing: boolean;
  detail: string;
};

export function sandboxSelftest(): Promise<SandboxProbe> {
  return invoke<SandboxProbe>(CMD_SANDBOX_SELFTEST);
}
