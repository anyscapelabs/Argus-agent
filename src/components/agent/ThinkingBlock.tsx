import { useState } from "react";
import { FiChevronDown } from "react-icons/fi";

import type { BlockNode } from "../../lib/agentXml";

type Props = { block: BlockNode };

export default function ThinkingBlock({ block }: Props) {
  const [open, setOpen] = useState(false);
  const body = block.children.map((c) => c.value).join("").trim();
  const raw = block.attrs.duration ?? block.attrs.elapsed ?? block.attrs.time;
  const elapsed = raw
    ? raw
    : `${Math.min(12, Math.max(2, Math.ceil(body.length / 40)))}s`;

  return (
    <div className="font-sans text-text-secondary">
      <button
        type="button"
        onClick={() => setOpen((v) => !v)}
        className={
          "flex w-fit items-center gap-1.5 text-text-secondary " +
          "transition-colors hover:text-text-primary focus:outline-none " +
          "focus-visible:text-text-primary"
        }
        aria-expanded={open}
      >
        <span className="text-[16px]">
          {open ? "Hide thoughts" : `Thoughts for ${elapsed}`}
        </span>
        <FiChevronDown
          size={14}
          className={`shrink-0 transition-transform ${open ? "" : "-rotate-90"}`}
        />
      </button>
      {open && (
        <div className="mt-1 font-serif text-[16px] font-light leading-6 text-text-secondary">
          {body || <span>thinking…</span>}
        </div>
      )}
    </div>
  );
}
