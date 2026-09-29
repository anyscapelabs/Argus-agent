// Background agent runs and the default-preferences record.
import { invoke } from "@tauri-apps/api/core";

export type AgentRun = {
  id: string;
  parentId: string;
  name: string;
  title: string;
  state: string;
  result: string | null;
  createdAt: string;
};

export function agentList(parentId: string): Promise<AgentRun[]> {
  return invoke<AgentRun[]>("agent_list", { parentId });
}

export type AgentRead = {
  run: AgentRun;
  text: string;
};

export function agentRead(id: string, maxChars?: number): Promise<AgentRead> {
  return invoke<AgentRead>("agent_read", { id, maxChars });
}

export function agentKill(id: string): Promise<boolean> {
  return invoke<boolean>("agent_kill", { id });
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
