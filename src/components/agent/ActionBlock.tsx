import { FiTerminal } from "react-icons/fi";

import type { BlockNode } from "../../lib/agentXml";

type Props = { block: BlockNode; live?: boolean };

const VERB: Record<string, string> = {
  "bash.run": "command",
  grep: "search",
  "fs.write": "file write",
};

export default function ActionBlock({ block, live = false }: Props) {
  const tool = block.attrs.tool ?? "";
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
          "min-w-0 max-w-[440px] truncate text-[13px] " +
          `${live ? "shimmer-text" : "text-text-secondary"}`
        }
      >
        {label}
      </span>
    </div>
  );
}
