import { useState } from "react";
import { FiList } from "react-icons/fi";

import type { BlockNode } from "../../lib/agentXml";

export default function ThoughtFold({ block }: { block: BlockNode }) {
  const [show, setShow] = useState(false);
  const body = block.children.map((c) => c.value).join("").trim();

  if (body.length === 0) return null;

  return (
    <div className="mt-2 flex flex-col gap-1 rounded-lg border border-border-primary px-3 py-2 first:mt-0">
      <button
        type="button"
        onClick={() => setShow((v) => !v)}
        className="flex items-center gap-2 text-left cursor-pointer"
      >
        <FiList size={13} className="shrink-0 text-text-secondary" />
        <span className="min-w-0 flex-1 truncate text-sm text-text-secondary">
          Thought
        </span>
        <span className="shrink-0 text-xs text-text-tertiary">
          {show ? "Hide ↑" : "Show →"}
        </span>
      </button>
      {show && (
        <p className="max-h-[240px] overflow-y-auto whitespace-pre-wrap pl-6 text-sm leading-6 text-text-secondary">
          {body}
        </p>
      )}
    </div>
  );
}
