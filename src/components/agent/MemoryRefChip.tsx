import { FaGithub } from "react-icons/fa";
import { FiBookmark } from "react-icons/fi";

import type { BlockNode } from "../../lib/agentXml";

type Props = { block: BlockNode };

export default function MemoryRefChip({ block }: Props) {
  const source = block.attrs.source;
  const date = block.attrs.date;
  const body = block.children.map((child) => child.value).join("").trim();
  if (!body && !source) return null;

  const isCommit = source === "commit";

  return (
    <div className="mt-4 flex flex-col gap-2 rounded-lg border border-border-primary bg-bg-secondary p-3 font-sans first:mt-0">
      <div className="flex items-center gap-2 text-text-secondary">
        {isCommit ? <FaGithub size={14} /> : <FiBookmark size={12} />}
        <span className="text-xs font-medium">
          {isCommit ? "Commit" : source}
        </span>
        {date && (
          <span className="text-xs text-text-secondary/60">· {date}</span>
        )}
      </div>
      {body && <div className="text-sm text-text-primary">{body}</div>}
    </div>
  );
}
