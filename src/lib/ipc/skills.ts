// Skills: the catalog, the editor, and the files inside one.
import { invoke } from "@tauri-apps/api/core";

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

export type NewSkill = {
  name: string;
  description: string;
  body: string;
  source?: string;
  origin?: string;
};

export function skillCreate(skill: NewSkill): Promise<Skill> {
  return invoke<Skill>("skill_create", { skill });
}

export function skillUpdate(
  name: string,
  upd: { description?: string; body?: string },
): Promise<Skill> {
  return invoke<Skill>("skill_update", { name, upd });
}

export function skillFiles(name: string): Promise<string[]> {
  return invoke<string[]>("skill_files", { name });
}

export function skillReadFile(name: string, file: string): Promise<string> {
  return invoke<string>("skill_read_file", { name, file });
}
