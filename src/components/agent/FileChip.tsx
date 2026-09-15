import type { BlockNode } from "../../lib/agentXml";

type Props = { block: BlockNode };

export default function FileChip({ block }: Props) {
  const path = block.attrs.path ?? "file";
  const fileName = path.split("/").pop() ?? path;

  return (
    <div className="flex items-center justify-between gap-3 py-0.5 font-sans text-sm">
      <span className="truncate font-mono text-text-secondary">
        {fileName}
      </span>
    </div>
  );
}
