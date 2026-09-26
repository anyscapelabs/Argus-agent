import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useState } from "react";
import { FiArrowLeft, FiUsers } from "react-icons/fi";

import { agentList, type AgentRun } from "../lib/ipc";
import { sessionStore, useSessions } from "../stores/sessions";
import AgentBubble from "./AgentBubble";
import UserBubble from "./UserBubble";

type Props = {
  agentId: string;
  parentId: string;
  onBack: () => void;
};

const VERDICT: Record<string, string> = {
  done: "Finished",
  failed: "Did not finish",
  interrupted: "Interrupted when Argus exited",
  killed: "Stopped",
  running: "Still working",
};

export default function SubagentPage({ agentId, parentId, onBack }: Props) {
  const { msgs, agents } = useSessions();
  const [run, setRun] = useState<AgentRun | null>(null);
  const [missing, setMissing] = useState(false);

  const reload = useCallback(async () => {
    try {
      const all = await agentList(parentId);
      const found = all.find((r) => r.id === agentId) ?? null;
      setRun(found);
      setMissing(found === null);
    } catch {
      setMissing(true);
    }
  }, [agentId, parentId]);

  useEffect(() => {
    void reload();
    void sessionStore.loadMsgs(agentId);

    const un = listen<{ id: string }>("agent-done", (e) => {
      if (e.payload.id !== agentId) {
        return;
      }

      void reload();
      void sessionStore.loadMsgs(agentId);
    });

    return () => {
      void un.then((f) => f());
    };
  }, [agentId, reload]);

  const rows = msgs[agentId] ?? [];
  const state = agents[agentId] ?? run?.state ?? "running";

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="flex shrink-0 items-center gap-2 border-b border-border-primary px-4 py-2.5">
        <button
          type="button"
          onClick={onBack}
          className="flex items-center gap-1.5 text-sm text-text-secondary hover:text-text-primary cursor-pointer"
        >
          <FiArrowLeft size={14} />
          Back
        </button>
        <FiUsers size={13} className="text-text-tertiary" />
        <span className="min-w-0 flex-1 truncate text-sm text-text-primary">
          {run?.name ?? "Sub-agent"}
        </span>
        <span className="shrink-0 text-xs text-text-secondary">
          {VERDICT[state] ?? state}
        </span>
      </div>

      {run !== null && run.title.length > 0 && (
        <div className="shrink-0 px-4 pt-3 text-sm text-text-secondary">
          {run.title}
        </div>
      )}

      <div className="min-h-0 flex-1 overflow-y-auto px-4 py-4">
        {missing ? (
          <p className="text-sm text-text-secondary">
            That sub-agent is no longer around.
          </p>
        ) : rows.length === 0 ? (
          <p className="text-sm text-text-secondary">
            {state === "running"
              ? "Working — nothing said yet."
              : "It said nothing."}
          </p>
        ) : (
          rows.map((m) =>
            m.role === "assistant" ? (
              <AgentBubble key={m.id} text={m.content} />
            ) : m.role === "tool" ? null : (
              <div key={m.id}>
                <div className="mb-1 text-xs text-text-tertiary">
                  {m.role === "system" ? "Prompt from Argus" : "You"}
                </div>
                <UserBubble>{m.content}</UserBubble>
              </div>
            ),
          )
        )}
      </div>

      <div className="shrink-0 border-t border-border-primary px-4 py-2.5 text-xs text-text-tertiary">
        This is a sub-agent's own transcript. It reports to the conversation
        that started it, not to you.
      </div>
    </div>
  );
}
