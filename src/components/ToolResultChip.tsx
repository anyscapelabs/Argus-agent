import { useState } from "react";
import { FiAlertTriangle, FiCheck, FiChevronDown, FiChevronRight, FiTerminal } from "react-icons/fi";

// <tool-result tool="..." status="ok|err">output</tool-result>, as persisted by the loop.
export function parseToolResult(raw: string): { tool: string; status: string; body: string } | null {
  const m = raw.match(/^<tool-result\s+tool="([^"]*)"\s+status="([^"]*)">([\s\S]*)<\/tool-result>$/);
  if (m === null) return null;
  return { tool: m[1], status: m[2], body: m[3].trim() };
}

export default function ToolResultChip({ raw }: { raw: string }) {
  const parsed = parseToolResult(raw);
  const [open, setOpen] = useState(false);
  if (parsed === null) return null;
  const ok = parsed.status === "ok";

  return (
    <div className="font-sans">
      <button
        type="button"
        onClick={() => setOpen((v) => !v)}
        className={`flex max-w-full items-center gap-2 rounded-lg border px-2.5 py-1.5 text-xs transition-colors ${
          ok
            ? "border-border-primary bg-bg-secondary text-text-secondary hover:text-text-primary"
            : "border-red-500/30 bg-red-500/10 text-red-400"
        }`}
      >
        {ok ? <FiCheck size={12} /> : <FiAlertTriangle size={12} />}
        <FiTerminal size={12} />
        <span className="font-mono">{parsed.tool}</span>
        <span className="max-w-[320px] truncate text-left opacity-70">{parsed.body.split("\n")[0]}</span>
        {open ? <FiChevronDown size={12} /> : <FiChevronRight size={12} />}
      </button>
      {open && (
        <pre className="mt-1 max-h-[280px] overflow-auto whitespace-pre-wrap break-words rounded-lg border border-border-primary bg-bg-secondary px-3 py-2 font-mono text-xs text-text-secondary">
          {parsed.body}
        </pre>
      )}
    </div>
  );
}
