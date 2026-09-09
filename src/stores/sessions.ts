import { Channel } from "@tauri-apps/api/core";
import { useSyncExternalStore } from "react";
import {
  sessChatStream,
  sessCleanDangling,
  sessCreateSession,
  sessDeleteSession,
  sessListMessages,
  sessListSessions,
  sessSaveSession,
  sessSetModel,
  sessSetPermission,
  sessSetVote,
  sessSupersedeFrom,
  type MsgRow,
  type SessionRow,
  type StreamEvent,
} from "../lib/ipc";

export type Turn = { text: string; err: string | null };

type State = {
  sessions: SessionRow[];
  loading: boolean;
  activeId: string | null;
  msgs: Record<string, MsgRow[]>;
  turns: Record<string, Turn>;
};


class SessionStore {
  private state: State = { sessions: [], loading: true, activeId: null, msgs: {}, turns: {} };
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

  async loadSessions() {
    try {
      const sessions = await sessListSessions();
      this.set({ sessions });
    } catch {
      this.set({ sessions: [] });
    }
    this.set({ loading: false });
  }

  async loadMsgs(sessionId: string) {
    try {
      const rows = await sessListMessages(sessionId);
      this.set({ msgs: { ...this.state.msgs, [sessionId]: rows } });
    } catch {
      this.set({ msgs: { ...this.state.msgs, [sessionId]: [] } });
    }
  }

  async select(sessionId: string | null) {
    this.set({ activeId: sessionId });
    if (sessionId === null) return;
    const t = this.state.turns[sessionId];
    if (t === undefined || t.err !== null) {
      try {
        await sessCleanDangling(sessionId);
      } catch {
        // Drop it
      }
    }
    await this.loadMsgs(sessionId);
  }

  async create(title: string, modelId: string | null, permission: string = "ask"): Promise<SessionRow> {
    const row = await sessCreateSession(title, modelId, permission);
    await this.loadSessions();
    return row;
  }

  async remove(sessionId: string) {
    await sessDeleteSession(sessionId);
    const msgs = { ...this.state.msgs };
    const turns = { ...this.state.turns };
    delete msgs[sessionId];
    delete turns[sessionId];
    this.set({
      msgs,
      turns,
      sessions: this.state.sessions.filter((s) => s.id !== sessionId),
      activeId: this.state.activeId === sessionId ? null : this.state.activeId,
    });
  }

  async setVote(sessionId: string, msgId: string, vote: "up" | "down" | null) {
    const rows = this.state.msgs[sessionId] ?? [];
    if (!rows.some((m) => m.id === msgId)) return;
    this.set({
      msgs: {
        ...this.state.msgs,
        [sessionId]: rows.map((m) => (m.id === msgId ? { ...m, vote } : m)),
      },
    });
    try {
      await sessSetVote(sessionId, msgId, vote);
    } catch {
      await this.loadMsgs(sessionId);
    }
  }

  // Optimistic column update; one retry, revert only if both fail.
  private async patchColumn(
    sessionId: string,
    field: "permission" | "model_id",
    value: string,
    apply: () => Promise<void>,
  ) {
    const row = this.state.sessions.find((s) => s.id === sessionId);
    if (row === undefined || row[field] === value) return;
    this.set({ sessions: this.state.sessions.map((s) => (s.id === sessionId ? { ...s, [field]: value } : s)) });
    try {
      await apply();
    } catch (e) {
      console.error("session update failed, retrying", e);
      try {
        await apply();
      } catch (e2) {
        console.error("session update failed, reverting", e2);
        this.set({
          sessions: this.state.sessions.map((s) => (s.id === sessionId ? { ...s, [field]: row[field] } : s)),
        });
      }
    }
  }

  async setPermission(sessionId: string, permission: string) {
    await this.patchColumn(sessionId, "permission", permission, () =>
      sessSetPermission(sessionId, permission),
    );
  }

  async setModel(sessionId: string, modelId: string) {
    await this.patchColumn(sessionId, "model_id", modelId, () => sessSetModel(sessionId, modelId));
  }

  async archive(sessionId: string) {
    const row = this.state.sessions.find((s) => s.id === sessionId);
    if (row === undefined) return;
    await sessSaveSession({ ...row, status: "archived" });
    await this.loadSessions();
  }

  private patchTurn(sessionId: string, patch: (prev: Turn) => Turn) {
    const prev = this.state.turns[sessionId];
    if (prev === undefined) return;
    this.set({ turns: { ...this.state.turns, [sessionId]: patch(prev) } });
  }

  private clearTurn(sessionId: string) {
    if (this.state.turns[sessionId] === undefined) return;
    const turns = { ...this.state.turns };
    delete turns[sessionId];
    this.set({ turns });
  }

  async send(sessionId: string, content: string) {
    const prev = this.state.turns[sessionId];
    if (prev !== undefined && prev.err === null) return;

    const pending: MsgRow = {
      id: `pending-${Date.now()}`,
      session_id: sessionId,
      seq: 0,
      role: "user",
      content,
      model_id: null,
      provider_id: null,
      tok_in: null,
      tok_out: null,
      active: true,
      vote: null,
      created_at: "",
    };
    const existing = this.state.msgs[sessionId] ?? [];
    this.set({ msgs: { ...this.state.msgs, [sessionId]: [...existing, pending] } });

    const row = this.state.sessions.find((s) => s.id === sessionId);
    if (row !== undefined && row.title === "New chat") {
      const tempTitle = content.trim().split("\n")[0].trim().slice(0, 60);
      if (tempTitle.length > 0) {
        this.set({
          sessions: this.state.sessions.map((s) =>
            s.id === sessionId ? { ...s, title: tempTitle } : s,
          ),
        });
      }
    }

    const chan = new Channel<StreamEvent>();
    chan.onmessage = (ev) => {
      if (ev.type === "delta") {
        this.patchTurn(sessionId, (prev) => ({ text: prev.text + ev.text, err: null }));
      } else if (ev.type === "reset") {
        this.patchTurn(sessionId, (prev) => ({ text: "", err: prev.err }));
      } else if (ev.type === "step") {
        this.set({ turns: { ...this.state.turns, [sessionId]: { text: "", err: null } } });
        void this.loadMsgs(sessionId);
      } else if (ev.type === "err") {
        this.patchTurn(sessionId, (prev) => ({ text: prev.text, err: ev.msg }));
      }
    };
    this.set({ turns: { ...this.state.turns, [sessionId]: { text: "", err: null } } });

    try {
      await sessChatStream(sessionId, content, chan);
      this.clearTurn(sessionId);
      await this.loadMsgs(sessionId);
      await this.loadSessions();
    } catch (e) {
      await this.loadMsgs(sessionId);
      this.set({ turns: { ...this.state.turns, [sessionId]: { text: "", err: String(e) } } });
    }
  }

  
  async retry(sessionId: string, userSeq: number, content: string) {
    const t = this.state.turns[sessionId];
    if (t !== undefined && t.err === null) return;
    try {
      await sessSupersedeFrom(sessionId, userSeq);
    } catch {
      // DB dead, resend still proceeds
    }
    await this.send(sessionId, content);
  }

  clearError(sessionId: string) {
    this.clearTurn(sessionId);
  }
}

export const sessionStore = new SessionStore();

export function useSessions(): State {
  return useSyncExternalStore(sessionStore.subscribe, sessionStore.getState);
}
