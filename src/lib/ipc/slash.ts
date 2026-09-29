// Slash commands and the usage figures the slash card shows.
import { invoke } from "@tauri-apps/api/core";


export type SlashCmd = {
  name: string;
  desc: string;
  kind: "local" | "prompt";
  arg: string | null;
};

export function slashList(): Promise<SlashCmd[]> {
  return invoke<SlashCmd[]>("slash_list");
}

// Exactly one of these is ever set: `text` is a line of the app speaking,
// `usage` is a card the chat draws, and `model` is a prompt macro sent in
// place of what was typed.
export type SlashOut = {
  text: string | null;
  model: string | null;
  usage: Usage | null;
};

export type UsageWindow = "today" | "week" | "month";

export type ModelUsage = {
  model: string;
  provider: string;
  requests: number;
  tokens: number;
  cost: number;
};

export type DayUsage = {
  day: string;
  requests: number;
  tokens: number;
  cost: number;
};

export type Usage = {
  window: UsageWindow;
  label: string;
  requests: number;
  failed: number;
  tokIn: number;
  tokOut: number;
  cost: number;
  workedMs: number;
  models: ModelUsage[];
  days: DayUsage[];
};

export function slashRun(
  name: string,
  sessionId: string | null,
  arg?: string,
): Promise<SlashOut> {
  return invoke<SlashOut>("slash_run", {
    name,
    sessionId,
    arg: arg ?? null,
  });
}
