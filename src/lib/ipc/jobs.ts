// Long-running jobs, as the task list renders them.
import { invoke } from "@tauri-apps/api/core";

export type Job = {
  id: string;
  session_id: string | null;
  label: string;
  command: string;
  cwd: string | null;
  profile: string;
  privilege: string;
  state: string;
  exit: number | null;
  wake: boolean;
  permission: string;
  started_ms: number;
  ended_ms: number | null;
  duration_ms: number | null;
  out_bytes: number;
  truncated: boolean;
  note: string | null;
};

export type JobRead = {
  job: Job;
  text: string;
};

export function jobList(sessionId?: string, limit?: number): Promise<Job[]> {
  return invoke<Job[]>("job_list", {
    sessionId: sessionId ?? null,
    limit: limit ?? null,
  });
}

export function jobRead(id: string, maxChars?: number): Promise<JobRead> {
  return invoke<JobRead>("job_read", { id, maxChars: maxChars ?? null });
}

export function jobKill(id: string): Promise<boolean> {
  return invoke<boolean>("job_kill", { id });
}
