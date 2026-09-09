import { FiGlobe, FiTerminal } from "react-icons/fi";
import type { BlockNode } from "../../lib/agentXml";

type Props = { block: BlockNode; live?: boolean };

const VERB: Record<string, string> = {
  "bash.run": "command",
  grep: "search",
  "fs.write": "file write",
};

const WEB = new Set(["web.search", "web.read"]);

// The action body is the model's JSON args; mid-stream it is still partial,
// so a failed parse falls back to the raw text.
function targetOf(tool: string, body: string): string {
  try {
    const args = JSON.parse(body) as { query?: string; url?: string };
    return (tool === "web.search" ? args.query : args.url) ?? body;
  } catch {
    return body;
  }
}

export default function ActionBlock({ block, live = false }: Props) {
  const tool = block.attrs.tool ?? "";
  const body = block.children.map((c) => c.value).join("").trim();
  const isWeb = WEB.has(tool);
  const icon = isWeb ? <FiGlobe size={11} /> : <FiTerminal size={11} />;
  const label = isWeb
    ? `${live ? (tool === "web.search" ? "Searching" : "Reading") : tool === "web.search" ? "Searched" : "Read"} ${targetOf(tool, body)}`
    : `${live ? "Running" : "Ran"} ${VERB[tool] ?? (tool || "command")}`;

  return (
    <div className="my-2 flex items-center gap-2.5 font-sans">
      <span className="flex h-5 w-5 shrink-0 items-center justify-center rounded-md border border-border-primary bg-bg-secondary text-text-secondary">
        {icon}
      </span>
      <span className={`min-w-0 max-w-[440px] truncate text-[13px] ${live ? "shimmer-text" : "text-text-secondary"}`}>
        {label}
      </span>
    </div>
  );
}
