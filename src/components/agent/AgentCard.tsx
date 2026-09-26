import { FiChevronRight, FiUsers } from "react-icons/fi";

import { agentKill } from "../../lib/ipc";
import { useSessions } from "../../stores/sessions";

type Props = {
  id: string;
  name: string;
  state?: string;
  children?: React.ReactNode;
  onOpen: (id: string) => void;
};

const VERDICT: Record<string, string> = {
  done: "Finished",
  failed: "Did not finish",
  interrupted: "Interrupted",
  killed: "Stopped",
  running: "Working",
};

export default function AgentCard({
  id,
  name,
  state = "running",
  children,
  onOpen,
}: Props) {
  const { agentRuns } = useSessions();
  const liveState = agentRuns[id]?.state ?? state;
  const live = liveState === "running";

  return (
    <div className="my-2 overflow-hidden rounded-lg border border-border-primary">
      <button
        type="button"
        onClick={() => onOpen(id)}
        className="flex w-full items-center gap-2.5 px-3 py-2 text-left cursor-pointer"
      >
        <span className="flex h-5 w-5 shrink-0 items-center justify-center rounded-md border border-border-primary text-text-secondary">
          <FiUsers size={11} />
        </span>
        <span
          className={
            "min-w-0 flex-1 truncate text-sm " +
            `${live ? "shimmer-text" : "text-text-primary"}`
          }
        >
          {name}
        </span>
        <span className="shrink-0 text-xs text-text-secondary">
          {VERDICT[liveState] ?? liveState}
        </span>
        <FiChevronRight size={12} className="shrink-0 text-text-tertiary" />
      </button>
      {children}
      {live && (
        <div className="border-t border-border-primary px-3 py-1.5 text-right">
          <button
            type="button"
            onClick={() => {
              void agentKill(id);
            }}
            className="cursor-pointer text-xs text-text-secondary hover:text-text-primary"
          >
            Stop this agent
          </button>
        </div>
      )}
    </div>
  );
}
