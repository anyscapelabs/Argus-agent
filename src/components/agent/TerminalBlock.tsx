import { useState } from "react";
import { FiChevronDown, FiTerminal } from "react-icons/fi";

import type { BlockNode } from "../../lib/agentXml";

type Props = { block: BlockNode };

export default function TerminalBlock({ block }: Props) {
  const [open, setOpen] = useState(true);
  const command = block.attrs.command ?? "shell";
  const body = block.children.map((child) => child.value).join("").trim();

  return (
    <div
      data-component-id={block.attrs.id}
      className="overflow-hidden rounded-lg border border-border-primary bg-bg-primary font-sans"
    >
      <button
        type="button"
        onClick={() => setOpen((value) => !value)}
        className={
          "flex w-full items-center gap-2 border-b border-border-primary " +
          "px-3 py-1.5 text-xs text-text-secondary transition-colors " +
          "hover:bg-bg-hover-primary focus:outline-none " +
          "focus-visible:bg-bg-hover-primary"
        }
        aria-expanded={open}
      >
        <FiTerminal size={14} />
        <span className="text-sm font-medium text-text-primary">Terminal</span>
        <FiChevronDown
          size={12}
          className={`ml-auto transition-transform ${open ? "" : "-rotate-90"}`}
        />
      </button>
      {open && (
        <>
          <div className="border-b border-border-primary px-3 py-2.5 font-mono text-sm leading-6">
            <span className="text-text-secondary">$ </span>
            <span className="text-text-primary">{command}</span>
          </div>
          <pre className="overflow-x-auto whitespace-pre px-3 py-2.5 font-mono text-sm leading-6 text-text-primary">
            {body || <span className="italic text-text-secondary">…</span>}
          </pre>
        </>
      )}
    </div>
  );
}
