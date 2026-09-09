import { useState } from "react";
import { FiAlertTriangle, FiCheck, FiChevronDown, FiChevronRight, FiLoader } from "react-icons/fi";
import type { BlockNode } from "../../lib/agentXml";

type Props = { block: BlockNode; live?: boolean };

type Status = "running" | "success" | "error";

export default function ActionBlock({ block, live = false }: Props) {
  const tool = block.attrs.tool ?? "action";
  const status = (block.attrs.status ?? (live ? "running" : "success")) as Status;
  const body = block.children.map((child) => child.value).join("").trim();
  const [open, setOpen] = useState(false);

  const mark =
    status === "running" ? (
      <FiLoader size={11} className="animate-spin text-text-secondary" />
    ) : status === "error" ? (
      <FiAlertTriangle size={11} className="text-red-400" />
    ) : (
      <FiCheck size={11} className="text-text-secondary" />
    );

  return (
    <div className="my-1 font-sans first:mt-0">
      <button
        type="button"
        onClick={() => setOpen((v) => !v)}
        className="group flex w-full items-center gap-1.5 rounded-md text-left text-xs text-text-secondary transition-colors hover:text-text-primary focus:outline-none"
      >
        {open ? <FiChevronDown size={11} /> : <FiChevronRight size={11} />}
        {mark}
        <span className="font-mono">{tool}</span>
        <span className="truncate font-mono opacity-60">{body.split("\n")[0]}</span>
      </button>
      {open && body && (
        <pre className="mt-1 max-h-[280px] overflow-auto whitespace-pre-wrap break-words border-l border-border-primary pl-3 font-mono text-xs text-text-secondary">
          {body}
        </pre>
      )}
    </div>
  );
}
