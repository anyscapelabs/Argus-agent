import { listen } from "@tauri-apps/api/event";
import { useEffect, useState } from "react";

import ChatTranscript from "./ChatTranscript";
import { sessionStore, useSessions } from "../stores/sessions";

type Props = {
  agentId: string;
  parentId: string;
  onBack: () => void;
};

const VERDICT: Record<string, string> = {
  done: "finished",
  failed: "did not finish",
  interrupted: "interrupted when Argus exited",
  killed: "stopped",
  running: "still working",
};

export default function SubagentPage({ agentId, parentId, onBack }: Props) {
  const { agentRuns, turns } = useSessions();
  const run = agentRuns[agentId] ?? null;
  const waiting = turns[agentId]?.approval ?? null;
  const [checked, setChecked] = useState(false);

  useEffect(() => {
    void sessionStore.loadMsgs(agentId);
    void sessionStore.loadAgents(parentId).finally(() => setChecked(true));
    sessionStore.watch(agentId);

    const un = listen<{ id: string }>("agent-done", (e) => {
      if (e.payload.id !== agentId) {
        return;
      }

      void sessionStore.loadMsgs(agentId);
      void sessionStore.loadAgents(parentId);
    });

    return () => {
      void un.then((f) => f());
      sessionStore.unwatch(agentId);
    };
  }, [agentId, parentId]);

  return (
    <div className="flex h-full min-h-0 flex-col">
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
              This sub-agent needs your approval to run a command. Approvals are
              answered in the chat that started it.
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

      {checked && run === null ? (
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
        {run === null
          ? "This is a sub-agent's own transcript."
          : `${VERDICT[run.state] ?? run.state} — this is the sub-agent's own transcript. It reports to the conversation that started it, not to you.`}
      </div>
    </div>
  );
}
