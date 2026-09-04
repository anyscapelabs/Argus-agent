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
        className="flex w-full items-center gap-2 border-b border-border-primary px-3 py-1.5 text-xs text-text-secondary transition-colors hover:bg-bg-hover-primary focus:outline-none focus-visible:bg-bg-hover-primary"
        aria-expanded={open}
      >
        <FiTerminal size={12} />
        <span className="font-mono text-text-primary">{command}</span>
        <FiChevronDown
          size={12}
          className={`ml-auto transition-transform ${open ? "" : "-rotate-90"}`}
        />
      </button>
      {open && (
        <pre className="overflow-x-auto whitespace-pre p-3 font-mono text-xs leading-5 text-text-primary">
          {body || <span className="italic text-text-secondary">…</span>}
        </pre>
      )}
    </div>
  );
}
