import { FiCheck, FiAlertTriangle, FiLoader } from "react-icons/fi";
import type { BlockNode } from "../../lib/agentXml";

type Props = { block: BlockNode; live?: boolean };

type Status = "running" | "success" | "error";

const STATUS_ICON: Record<Status, React.ReactNode> = {
  running: <FiLoader size={12} className="animate-spin" />,
  success: <FiCheck size={12} />,
  error: <FiAlertTriangle size={12} />,
};

export default function ActionBlock({ block, live = false }: Props) {
  const tool = block.attrs.tool ?? "action";
  const status = (block.attrs.status ?? (live ? "running" : "success")) as Status;
  const body = block.children.map((child) => child.value).join("").trim();

  return (
    <div className="flex flex-col gap-1.5 rounded-lg border border-border-primary bg-bg-secondary p-3 font-sans">
      <div className="flex items-center gap-2">
        <span className="font-mono text-xs text-text-secondary">{tool}</span>
        <span className="ml-auto flex items-center gap-1 text-text-secondary">
          {STATUS_ICON[status]}
          <span className="text-xs">{status}</span>
        </span>
      </div>
      {body && <div className="text-sm text-text-primary">{body}</div>}
    </div>
  );
}
