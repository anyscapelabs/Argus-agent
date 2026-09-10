import { useEffect, useRef, useState } from "react";
import { FiChevronDown, FiTerminal } from "react-icons/fi";

import type { BlockNode } from "../../lib/agentXml";
import { sessionStore, type PendingApproval } from "../../stores/sessions";

type Props = {
  block: BlockNode;
  live?: boolean;
  output?: string;
  code?: number;
  approval?: PendingApproval | null;
  sessionId?: string;
};

const VERB: Record<string, string> = {
  terminal: "command",
  "bash.run": "command",
  grep: "search",
  "fs.write": "file write",
};

function statusBadge(code: number | undefined, hasOutput: boolean) {
  if (code === undefined) {
    if (!hasOutput) return null;
    return <span className="shimmer-text text-xs">running</span>;
  }

  if (code === 0) return null;

  const label = code === -1 ? "timed out" : code === -2 ? "denied" : `exit ${code}`;

  return <span className="text-xs text-red-400">{label}</span>;
}

export default function ActionBlock({
  block,
  live = false,
  output,
  code,
  approval,
  sessionId,
}: Props) {
  const [open, setOpen] = useState(true);
  const preRef = useRef<HTMLPreElement | null>(null);

  const tool = block.attrs.tool ?? "";
  const isTerm = tool === "terminal" || tool === "bash.run";

  useEffect(() => {
    const el = preRef.current;
    if (el) {
      el.scrollTop = el.scrollHeight;
    }
  }, [output]);

  if (!isTerm) {
    const label = `${live ? "Running" : "Ran"} ${VERB[tool] ?? (tool || "command")}`;

    return (
      <div className="my-2 flex items-center gap-2.5 font-sans">
        <span
          className={
            "flex h-5 w-5 shrink-0 items-center justify-center rounded-md " +
            "border border-border-primary bg-bg-secondary text-text-secondary"
          }
        >
          <FiTerminal size={11} />
        </span>
        <span
          className={
            "min-w-0 max-w-[440px] truncate text-sm " +
            `${live ? "shimmer-text" : "text-text-secondary"}`
          }
        >
          {label}
        </span>
      </div>
    );
  }

  const argsRaw = block.children.map((c) => c.value).join("").trim();

  let command = "";
  try {
    command = (JSON.parse(argsRaw)["command"] as string) ?? "";
  } catch {
    command = argsRaw || "shell";
  }

  const decide = (allow: boolean) => {
    if (sessionId !== undefined) {
      sessionStore.resolveApproval(sessionId, allow);
    }
  };

  return (
    <div className="my-2 overflow-hidden rounded-lg border border-border-primary bg-bg-primary font-sans">
      <button
        type="button"
        onClick={() => setOpen((v) => !v)}
        aria-expanded={open}
        className={
          "flex w-full items-center gap-2 border-b border-border-primary " +
          "px-3 py-1.5 text-xs transition-colors hover:bg-bg-hover-primary " +
          "focus:outline-none focus-visible:bg-bg-hover-primary"
        }
      >
        <FiTerminal size={12} className="shrink-0 text-text-secondary" />
        <span className="text-left font-medium text-text-primary">
          Terminal
        </span>
        {statusBadge(code, output !== undefined)}
        <FiChevronDown
          size={12}
          className={`ml-auto shrink-0 transition-transform ${open ? "" : "-rotate-90"}`}
        />
      </button>
      {approval !== undefined && approval !== null && (
        <div className="flex items-center gap-2 border-b border-border-primary px-3 py-2">
          <span className="text-xs text-text-secondary">Run this command?</span>
          <button
            type="button"
            onClick={() => decide(true)}
            className="rounded-full bg-white px-3 py-1 text-xs font-medium text-bg-primary hover:opacity-90"
          >
            Run
          </button>
          <button
            type="button"
            onClick={() => decide(false)}
            className="rounded-full border border-border-primary px-3 py-1 text-xs text-text-secondary hover:text-text-primary"
          >
            Deny
          </button>
        </div>
      )}
      {open && (
        <>
          <div className="border-b border-border-primary px-3 py-2 font-mono text-xs">
            <span className="text-text-secondary">$ </span>
            <span className="text-text-primary">{command}</span>
          </div>
          {output !== undefined && (
            <pre
              ref={preRef}
              className="max-h-[240px] overflow-y-auto whitespace-pre-wrap p-3 font-mono text-xs leading-5 text-text-primary"
            >
              {output.length > 0 ? output : "…"}
            </pre>
          )}
        </>
      )}
    </div>
  );
}
