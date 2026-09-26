import { useSyncExternalStore } from "react";

import {
  profileActive,
  profileCreate,
  profileDelete,
  profileEdit,
  profileList,
  profileReachGet,
  profileReachSet,
  profileSetActive,
  sessGetProfile,
  sessSetProfile,
  type Grant,
  type ProfileRow,
  type Reach,
} from "../lib/ipc";

const DEFAULT_ID = "default";

/// What the picker shows for a profile with no name. The default exists before
/// anyone names anything, and a blank row reads as a broken one.
export function profileLabel(p: { name: string }): string {
  return p.name.trim() === "" ? "Default" : p.name.trim();
}

/// The reach ladder, weakest rung first. The key is what the database and the
/// command call it; the label is what a person is agreeing to. Each rung hands
/// over strictly more than the one above it, which is why they read top down.
export const CAPABILITIES: { key: Grant["capability"]; label: string; note: string }[] = [
  {
    key: "see_activity",
    label: "See what they are doing",
    note: "Chats, tools, plans — the live view. Read only.",
  },
  {
    key: "read_chats",
    label: "Read their chats",
    note: "The full history, not just the titles.",
  },
  {
    key: "write_prompts",
    label: "Write to their prompt",
    note: "Add instructions this profile will answer under.",
  },
  {
    key: "interrupt",
    label: "Interrupt their work",
    note: "Stop a running turn and take it over.",
  },
  {
    key: "edit",
    label: "Edit their profile",
    note: "Rename them and rewrite what they are told to do.",
  },
  {
    key: "change_access",
    label: "Change who they can reach",
    note: "The keys to the keys. Widens their reach further.",
  },
];

export const EMPTY_REACH: Reach = { reach_all: false, grants: [] };

type State = {
  profiles: ProfileRow[];
  loading: boolean;
  /// Which profile a chat with no row of its own belongs to. The toolbar
  /// picker sets this before a chat exists, and a new chat starts on it.
  activeId: string;
  /// Which profile the open chat belongs to, once one is open. Null means the
  /// chat has not asked yet.
  sessionId: string | null;
  /// The matrix of the profile whose detail page is open. Null until it asks.
  reach: Reach;
  reachFor: string | null;
};

class ProfileStore {
  private state: State = {
    profiles: [],
    loading: true,
    activeId: DEFAULT_ID,
    sessionId: null,
    reach: EMPTY_REACH,
    reachFor: null,
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
      const [profiles, saved] = await Promise.all([
        profileList(),
        profileActive(),
      ]);
      // The last pick survives a restart, unless the profile it named is
      // gone — a stale id would scope every list and prompt to nothing.
      const known = profiles.some((p) => p.id === saved);

      this.set({
        profiles,
        activeId: known ? saved : DEFAULT_ID,
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
    await this.setActive(p.id);
    await this.load();
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
  async setActive(id: string) {
    this.set({ activeId: id });
    await profileSetActive(id).catch(() => {});
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
    // A chat you moved is a choice, and choices are what survive a restart.
    await profileSetActive(profileId).catch(() => {});
  }

  async loadReach(id: string) {
    this.set({ reachFor: id });

    try {
      this.set({ reach: await profileReachGet(id) });
    } catch {
      this.set({ reach: EMPTY_REACH });
    }
  }

  async saveReach(id: string, reachAll: boolean, grants: Grant[]) {
    this.set({ reach: { reach_all: reachAll, grants } });
    await profileReachSet(id, reachAll, grants);
    // reach_all rides along on the profile row the list draws from.
    await this.load();
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
