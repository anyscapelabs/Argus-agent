import { Channel } from "@tauri-apps/api/core";
import { useSyncExternalStore } from "react";
import {
  sessChatStream,
  sessCreateSession,
  sessDeleteSession,
  sessListMessages,
  sessListSessions,
  sessSaveSession,
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

// Module-level store: running turns live here, not in component state, so a
// turn keeps streaming while its chat view is unmounted. Reopening the
// session shows the buffered text live and the persisted rows on completion.
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
    if (sessionId !== null) await this.loadMsgs(sessionId);
  }

  async create(title: string, modelId: string | null): Promise<SessionRow> {
    const row = await sessCreateSession(title, modelId);
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
    if (this.state.turns[sessionId] !== undefined) return;

    const chan = new Channel<StreamEvent>();
    chan.onmessage = (ev) => {
      if (ev.type === "delta") {
        this.patchTurn(sessionId, (prev) => ({ text: prev.text + ev.text, err: null }));
      } else if (ev.type === "err") {
        this.patchTurn(sessionId, (prev) => ({ text: prev.text, err: ev.msg }));
      }
    };
    this.set({ turns: { ...this.state.turns, [sessionId]: { text: "", err: null } } });

    try {
      await sessChatStream(sessionId, content, chan);
      await this.loadMsgs(sessionId);
      await this.loadSessions();
      this.clearTurn(sessionId);
    } catch (e) {
      await this.loadMsgs(sessionId);
      this.patchTurn(sessionId, (prev) => ({ text: prev.text, err: String(e) }));
    }
  }

  // Marks everything after the user turn inactive, then resends the content
  // as a fresh user message.
  async retry(sessionId: string, userSeq: number, content: string) {
    if (this.state.turns[sessionId] !== undefined) return;
    try {
      await sessSupersedeFrom(sessionId, userSeq + 1);
    } catch {
      // history stays as-is; the resend still proceeds
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
