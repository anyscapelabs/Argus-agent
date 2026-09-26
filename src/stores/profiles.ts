import { useSyncExternalStore } from "react";

import {
  profileCreate,
  profileDelete,
  profileEdit,
  profileList,
  sessGetProfile,
  sessSetProfile,
  type ProfileRow,
} from "../lib/ipc";

const DEFAULT_ID = "default";

/// What the picker shows for a profile with no name. The default exists before
/// anyone names anything, and a blank row reads as a broken one.
export function profileLabel(p: ProfileRow): string {
  return p.name.trim() === "" ? "Default" : p.name.trim();
}

type State = {
  profiles: ProfileRow[];
  loading: boolean;
  /// Which profile a chat with no row of its own belongs to. The toolbar
  /// picker sets this before a chat exists, and a new chat starts on it.
  activeId: string;
  /// Which profile the open chat belongs to, once one is open. Null means the
  /// chat has not asked yet.
  sessionId: string | null;
};

class ProfileStore {
  private state: State = {
    profiles: [],
    loading: true,
    activeId: DEFAULT_ID,
    sessionId: null,
  };

  private listeners = new Set<() => void>();

  subscribe = (fn: () => void) => {
    this.listeners.add(fn);
    return () => {
      this.listeners.delete(fn);
    };
  };

  getState = () => this.state;

  private set(patch: Partial<State>) {
    this.state = { ...this.state, ...patch };
    for (const fn of this.listeners) fn();
  }

  async load() {
    try {
      const profiles = await profileList();
      const known = profiles.some((p) => p.id === this.state.activeId);

      this.set({
        profiles,
        // A profile the user deleted out from under the picker must not stay
        // selected, or every new chat lands on a row that is not there.
        activeId: known ? this.state.activeId : DEFAULT_ID,
        loading: false,
      });
    } catch {
      this.set({ profiles: [], loading: false });
    }
  }

  byId(id: string): ProfileRow | undefined {
    return this.state.profiles.find((p) => p.id === id);
  }

  async create(name: string): Promise<ProfileRow> {
    const p = await profileCreate(name);
    await this.load();
    this.set({ activeId: p.id });
    return p;
  }

  async edit(
    id: string,
    patch: { name?: string; instructions?: string },
  ): Promise<ProfileRow> {
    const p = await profileEdit(id, patch);
    await this.load();
    return p;
  }

  async remove(id: string) {
    await profileDelete(id);
    await this.load();
  }

  /// Picks the profile a new chat starts on. Separate from picking the one an
  /// open chat belongs to, so the two never fight over the same field.
  setActive(id: string) {
    this.set({ activeId: id });
  }

  /// Reads what an open chat is actually on. A chat that has never been
  /// touched follows whatever the picker last chose.
  async loadForSession(sessionId: string) {
    this.set({ sessionId });

    try {
      const id = await sessGetProfile(sessionId);
      this.set({ activeId: id ?? DEFAULT_ID });
    } catch {
      this.set({ activeId: DEFAULT_ID });
    }
  }

  async setForSession(sessionId: string, profileId: string) {
    await sessSetProfile(sessionId, profileId);
    this.set({ activeId: profileId });
  }
}

export const profileStore = new ProfileStore();

export function useProfiles(): State {
  return useSyncExternalStore(
    profileStore.subscribe,
    profileStore.getState,
    profileStore.getState,
  );
}
