// Profiles, the folders they may reach, and which profile a session uses.
import { invoke } from "@tauri-apps/api/core";


export type ProfileRow = {
  id: string;
  name: string;
  instructions: string;
  reach_all: boolean;
  grants: number;
  created_at: string;
};

export function profileList(): Promise<ProfileRow[]> {
  return invoke<ProfileRow[]>("profile_list");
}

export function profileCreate(name: string): Promise<ProfileRow> {
  return invoke<ProfileRow>("profile_create", { name });
}

export function profileEdit(
  id: string,
  patch: { name?: string; instructions?: string },
): Promise<ProfileRow> {
  return invoke<ProfileRow>("profile_edit", {
    id,
    name: patch.name ?? null,
    instructions: patch.instructions ?? null,
  });
}

export function profileDelete(id: string): Promise<void> {
  return invoke<void>("profile_delete", { id });
}

export function profileActive(): Promise<string> {
  return invoke<string>("profile_active");
}

export function profileSetActive(id: string): Promise<void> {
  return invoke<void>("profile_set_active", { id });
}

export function sessSetProfile(
  sessionId: string,
  profileId: string,
): Promise<void> {
  return invoke<void>("sess_set_profile", { sessionId, profileId });
}

export function sessGetProfile(sessionId: string): Promise<string | null> {
  return invoke<string | null>("sess_get_profile", { sessionId });
}

export type Grant = {
  capability: string;
  target_id: string;
};

export type Reach = {
  reach_all: boolean;
  grants: Grant[];
};

export function profileReachGet(profileId: string): Promise<Reach> {
  return invoke<Reach>("profile_reach_get", { profileId });
}

export function profileReachSet(
  profileId: string,
  reachAll: boolean,
  grants: Grant[],
): Promise<void> {
  return invoke<void>("profile_reach_set", { profileId, reachAll, grants });
}
