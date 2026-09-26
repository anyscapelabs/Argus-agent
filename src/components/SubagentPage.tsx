import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useState } from "react";
import { FiArrowLeft, FiUsers } from "react-icons/fi";

import ChatTranscript from "./ChatTranscript";
import { agentList, type AgentRun } from "../lib/ipc";
import { sessionStore, useSessions } from "../stores/sessions";

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
  const [run, setRun] = useState<AgentRun | null>(null);
  const [missing, setMissing] = useState(false);
  const { turns } = useSessions();
  const waiting = turns[agentId]?.approval ?? null;

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
    sessionStore.watch(agentId);

    const un = listen<{ id: string }>("agent-done", (e) => {
      if (e.payload.id !== agentId) {
        return;
      }

      void reload();
      void sessionStore.loadMsgs(agentId);
    });

    return () => {
      void un.then((f) => f());
      sessionStore.unwatch(agentId);
    };
  }, [agentId, reload]);

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="flex shrink-0 items-center gap-2 border-b border-border-primary px-4 py-2.5">
        <button
          type="button"
          onClick={onBack}
          className="flex cursor-pointer items-center gap-1.5 text-sm text-text-secondary hover:text-text-primary"
        >
          <FiArrowLeft size={14} />
          Back
        </button>
        <FiUsers size={13} className="text-text-tertiary" />
        <span className="min-w-0 flex-1 truncate text-sm text-text-primary">
          {run?.name ?? "Sub-agent"}
        </span>
        <span className="shrink-0 text-xs text-text-secondary">
          {VERDICT[run?.state ?? "running"] ?? run?.state}
        </span>
      </div>

      {run !== null && run.title.length > 0 && (
        <div className="shrink-0 px-6 pt-4 text-sm text-text-secondary">
          <div className="mx-auto w-full min-w-0 max-w-[700px]">{run.title}</div>
        </div>
      )}

      {waiting !== null && (
        <div className="shrink-0 px-6 pt-3">
          <div
            className={
              "mx-auto flex w-full min-w-0 max-w-[700px] items-center gap-2 " +
              "rounded-lg border border-border-primary px-3 py-2 text-xs " +
              "text-text-secondary"
            }
          >
            <span className="min-w-0 flex-1">
              This sub-agent needs your approval to run a command. Approvals
              are answered in the chat that started it.
            </span>
            <button
              type="button"
              onClick={onBack}
              className="shrink-0 cursor-pointer text-text-primary underline"
            >
              Go there
            </button>
          </div>
        </div>
      )}

      {missing ? (
        <div className="min-h-0 flex-1 overflow-y-auto px-6 py-6">
          <p className="mx-auto w-full min-w-0 max-w-[700px] text-sm text-text-secondary">
            That sub-agent is no longer around.
          </p>
        </div>
      ) : (
        <ChatTranscript
          sessionId={agentId}
          readOnly
          userLabel="Prompt from Argus"
        />
      )}

      <div className="shrink-0 border-t border-border-primary px-4 py-2.5 text-xs text-text-tertiary">
        This is a sub-agent's own transcript. It reports to the conversation
        that started it, not to you.
      </div>
    </div>
  );
}
