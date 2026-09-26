import { Channel } from "@tauri-apps/api/core";
import { useSyncExternalStore } from "react";

import {
  agentList,
  sessCancelChat,
  sessChatStream,
  sessCleanDangling,
  sessCreateSession,
  sessDeleteSession,
  sessListMessages,
  sessListSessions,
  sessResolveApproval,
  sessSaveSession,
  sessSetModel,
  sessSetPermission,
  sessSetVote,
  sessSetWebSearch,
  sessSupersedeFrom,
  sessUnwatch,
  sessWatchEvents,
  type AgentRun,
  type MsgRow,
  type SessionRow,
  type StreamEvent,
} from "../lib/ipc";

export type PendingApproval = {
  id: string;
  idx: number;
  command: string;
};

export type Turn = {
  text: string;
  err: string | null;
  term: Record<number, string>;
  termCode: Record<number, number>;
  approval: PendingApproval | null;
  status: TurnStatus | null;
};

export type TurnStatus = { providerId: string; attempt: number };

const EMPTY_TXT = "";
const DEF_TITLE = "New chat";
const TITLE_CLIP = 60;
const PENDING_PREFIX = "pending-";

class SessError extends Error {}
class SessRetryError extends SessError {}

function blankTurn(err: string | null = null): Turn {
  return {
    text: EMPTY_TXT,
    err,
    term: {},
    termCode: {},
    approval: null,
    status: null,
  };
}

type State = {
  sessions: SessionRow[];
  loading: boolean;
  activeId: string | null;
  msgs: Record<string, MsgRow[]>;
  turns: Record<string, Turn>;
  stopped: Record<string, boolean>;
  agentRuns: Record<string, AgentRun>;
};

/// A conversation is not finished while a sub-agent is still working under it.
/// A parent's turn ending is not the end of the job — it promised to report
/// when the children finish, and the children are what knows they have not.
export function isWorking(state: State, id: string): boolean {
  if (state.turns[id] !== undefined) {
    return true;
  }

  const row = state.sessions.find((s) => s.id === id);

  return row !== undefined && row.running_agents > 0;
}

class SessionStore {
  private state: State = {
    sessions: [],
    loading: true,
    activeId: null,
    msgs: {},
    turns: {},
    stopped: {},
    agentRuns: {},
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

  onTurnStart: ((sessionId: string) => void) | null = null;
  onTurnDone:
    ((sessionId: string, ok: boolean, snippet: string) => void) | null = null;

  async loadSessions() {
    try {
      const sessions = await sessListSessions();
      this.set({ sessions });
    } catch {
      this.set({ sessions: [] });
    }

    this.set({ loading: false });
  }

  async loadAgents(sessionId: string) {
    try {
      const runs = await agentList(sessionId);

      if (runs.length === 0) {
        return;
      }

      const agentRuns = { ...this.state.agentRuns };

      for (const r of runs) {
        agentRuns[r.id] = r;
      }

      this.set({ agentRuns });
    } catch {}
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
    if (this.state.activeId !== sessionId) this.unwatch(this.state.activeId);
    this.set({ activeId: sessionId });

    if (sessionId === null) return;

    const t = this.state.turns[sessionId];
    if (t === undefined || t.err !== null) {
      try {
        await sessCleanDangling(sessionId);
      } catch {}
    }

    await this.loadMsgs(sessionId);
    await this.loadAgents(sessionId);
    this.watch(sessionId);
  }

  async create(
    title: string,
    modelId: string | null,
    permission: string = "ask",
    webSearch: boolean = false,
  ): Promise<SessionRow> {
    const row = await sessCreateSession(title, modelId, permission, webSearch);
    await this.loadSessions();
    return row;
  }

  async remove(sessionId: string) {
    await sessDeleteSession(sessionId);

    const msgs = { ...this.state.msgs };
    const turns = { ...this.state.turns };
    const stopped = { ...this.state.stopped };
    delete msgs[sessionId];
    delete turns[sessionId];
    delete stopped[sessionId];

    this.set({
      msgs,
      turns,
      stopped,
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

  private async patchColumn(
    sessionId: string,
    field: "permission" | "model_id" | "web_search",
    value: string | boolean | null,
    apply: () => Promise<void>,
  ) {
    const row = this.state.sessions.find((s) => s.id === sessionId);
    if (row === undefined || row[field] === value) return;

    this.set({
      sessions: this.state.sessions.map((s) =>
        s.id === sessionId ? { ...s, [field]: value } : s,
      ),
    });

    try {
      await apply();
    } catch (err) {
      try {
        await apply();
      } catch (err2) {
        this.set({
          sessions: this.state.sessions.map((s) =>
            s.id === sessionId ? { ...s, [field]: row[field] } : s,
          ),
        });
      }
    }
  }

  async setPermission(sessionId: string, permission: string) {
    await this.patchColumn(sessionId, "permission", permission, () =>
      sessSetPermission(sessionId, permission),
    );
  }

  async setModel(sessionId: string, modelId: string | null) {
    await this.patchColumn(sessionId, "model_id", modelId, () =>
      sessSetModel(sessionId, modelId),
    );
  }

  async setWebSearch(sessionId: string, on: boolean) {
    await this.patchColumn(sessionId, "web_search", on, () =>
      sessSetWebSearch(sessionId, on),
    );
  }

  async archive(sessionId: string) {
    const row = this.state.sessions.find((s) => s.id === sessionId);
    if (row === undefined) throw new SessRetryError("sess gone");

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

  private apply(sessionId: string, ev: StreamEvent) {
    const patch = (fn: (prev: Turn) => Turn) => {
      const prev = this.state.turns[sessionId] ?? blankTurn();
      this.set({ turns: { ...this.state.turns, [sessionId]: fn(prev) } });
    };

    if (ev.type === "status") {
      patch((prev) => ({
        ...prev,
        status: { providerId: ev.provider_id, attempt: ev.attempt },
      }));
      return;
    }

    if (ev.type === "delta") {
      patch((prev) => ({ ...prev, text: prev.text + ev.text, err: null }));
      return;
    }

    if (ev.type === "reset") {
      patch((prev) => ({ ...blankTurn(), err: prev.err }));
      return;
    }

    if (ev.type === "step") {
      this.set({ turns: { ...this.state.turns, [sessionId]: blankTurn() } });
      void this.loadMsgs(sessionId);
      return;
    }

    if (ev.type === "term") {
      patch((prev) => ({
        ...prev,
        term: { ...prev.term, [ev.idx]: (prev.term[ev.idx] ?? "") + ev.chunk },
      }));
      return;
    }

    if (ev.type === "term_end") {
      patch((prev) => ({
        ...prev,
        termCode: { ...prev.termCode, [ev.idx]: ev.code },
        approval: prev.approval?.idx === ev.idx ? null : prev.approval,
      }));
      return;
    }

    if (ev.type === "approval") {
      patch((prev) => ({
        ...prev,
        approval: { id: ev.id, idx: ev.idx, command: ev.command },
      }));
      return;
    }

    if (ev.type === "notice") {
      patch((prev) => ({
        ...prev,
        text: prev.text + `\n\n<warning severity="medium">${ev.msg}</warning>`,
      }));
      return;
    }

    if (ev.type === "err") {
      patch((prev) => ({ ...prev, err: ev.msg }));
      return;
    }

    if (ev.type === "refresh") {
      void this.loadMsgs(sessionId);
      void this.loadAgents(sessionId);
      // A sub-agent just landed. The count of the ones still working is what
      // keeps this conversation reading as unfinished, so it has to be re-read.
      void this.loadSessions();
      return;
    }

    if (ev.type === "turn_end") {
      this.clearTurn(sessionId);
      void this.loadMsgs(sessionId);
      void this.loadSessions();
    }
  }

  private watching = new Map<string, number>();
  private watchSeq = 0;

  watch(sessionId: string) {
    if (this.watching.has(sessionId)) return;

    const gen = ++this.watchSeq;
    this.watching.set(sessionId, gen);

    const chan = new Channel<StreamEvent>();

    chan.onmessage = (ev) => {
      if (this.watching.get(sessionId) !== gen) return;
      this.apply(sessionId, ev);
    };

    void sessWatchEvents(sessionId, chan).finally(() => {
      if (this.watching.get(sessionId) === gen) this.watching.delete(sessionId);
    });
  }

  unwatch(sessionId: string | null) {
    if (sessionId !== null) this.watching.delete(sessionId);
    else this.watching.clear();

    void sessUnwatch(sessionId ?? undefined).catch(() => {});
  }

  async send(sessionId: string, content: string) {
    const prev = this.state.turns[sessionId];
    if (prev !== undefined && prev.err === null) return;

    const pending: MsgRow = {
      id: `${PENDING_PREFIX}${Date.now()}`,
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
    this.set({
      msgs: { ...this.state.msgs, [sessionId]: [...existing, pending] },
    });

    const row = this.state.sessions.find((s) => s.id === sessionId);
    if (row !== undefined && row.title === DEF_TITLE) {
      const tempTitle = content
        .trim()
        .split("\n")[0]
        .trim()
        .slice(0, TITLE_CLIP);
      if (tempTitle.length > 0) {
        this.set({
          sessions: this.state.sessions.map((s) =>
            s.id === sessionId ? { ...s, title: tempTitle } : s,
          ),
        });
      }
    }

    const chan = new Channel<StreamEvent>();

    chan.onmessage = (ev) => this.apply(sessionId, ev);

    this.unwatch(sessionId);

    this.set({
      turns: { ...this.state.turns, [sessionId]: blankTurn() },
      stopped: { ...this.state.stopped, [sessionId]: false },
    });
    this.onTurnStart?.(sessionId);

    try {
      await sessChatStream(sessionId, content, chan);
      const snippet = (this.state.turns[sessionId]?.text ?? "")
        .split("\n")[0]
        .trim()
        .slice(0, 120);
      this.clearTurn(sessionId);
      await this.loadMsgs(sessionId);
      await this.loadSessions();
      this.onTurnDone?.(sessionId, true, snippet);
    } catch (err) {
      await this.loadMsgs(sessionId);

      if (String(err).includes("stopped")) {
        this.clearTurn(sessionId);
        return;
      }

      this.set({
        turns: { ...this.state.turns, [sessionId]: blankTurn(String(err)) },
      });
      this.onTurnDone?.(sessionId, false, String(err).slice(0, 120));
    }

    if (this.state.activeId === sessionId) this.watch(sessionId);
  }

  async resolveApproval(sessionId: string, allow: boolean, args?: string) {
    const t = this.state.turns[sessionId];
    if (t === undefined || t.approval === null) return;

    const { id } = t.approval;
    this.patchTurn(sessionId, (prev) => ({ ...prev, approval: null }));

    try {
      await sessResolveApproval(id, allow, args);
    } catch {}
  }

  async retry(sessionId: string, usrSeq: number, content: string) {
    const t = this.state.turns[sessionId];
    if (t !== undefined && t.err === null) return;

    try {
      await sessSupersedeFrom(sessionId, usrSeq);
    } catch {}

    await this.send(sessionId, content);
  }

  async stop(sessionId: string) {
    try {
      await sessCancelChat(sessionId);
    } catch {}

    this.clearTurn(sessionId);
    this.set({ stopped: { ...this.state.stopped, [sessionId]: true } });
    await this.loadMsgs(sessionId);
  }

  clearErr(sessionId: string) {
    this.clearTurn(sessionId);
  }
}

export const sessionStore = new SessionStore();

export function useSessions(): State {
  return useSyncExternalStore(sessionStore.subscribe, sessionStore.getState);
}
