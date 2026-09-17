import { useEffect, useState } from "react";
import { FiChevronDown, FiTerminal, FiTool } from "react-icons/fi";
import { SiGooglechrome } from "react-icons/si";

import { sessionStore, type PendingApproval } from "../../stores/sessions";

import type { BlockNode } from "../../lib/agentXml";

function argsOf(blk: BlockNode): Record<string, unknown> {
  try {
    return JSON.parse(blk.children.map((c) => c.value).join("")) as Record<
      string,
      unknown
    >;
  } catch {
    return {};
  }
}

function strArg(args: Record<string, unknown>, key: string): string {
  const v = args[key];
  return typeof v === "string" ? v : "";
}

function numArg(args: Record<string, unknown>, key: string): string {
  const v = args[key];
  return typeof v === "number" ? String(v) : "";
}

function short(s: string, n = 60): string {
  return s.length > n ? `${s.slice(0, n)}…` : s;
}

function browserLiveLabel(tool: string, args: Record<string, unknown>): string {
  if (tool === "browser.open") return `Opening ${strArg(args, "url")}`;
  if (tool === "browser.click") return `Clicking ${numArg(args, "ref")}`;
  if (tool === "browser.type") return `Typing ${numArg(args, "ref")}`;
  if (tool === "browser.read") return "Reading page";
  if (tool === "browser.scroll")
    return `Scrolling ${strArg(args, "direction") || "down"}`;
  if (tool === "browser.close") return "Closing tab";
  return tool || "Browsing";
}

function browserDoneLabel(tool: string, args: Record<string, unknown>): string {
  if (tool === "browser.open") return `Opened ${strArg(args, "url")}`;
  if (tool === "browser.click") return `Clicked ${numArg(args, "ref")}`;
  if (tool === "browser.type") return `Typed ${numArg(args, "ref")}`;
  if (tool === "browser.read") return "Read page";
  if (tool === "browser.scroll")
    return `Scrolled ${strArg(args, "direction") || "down"}`;
  if (tool === "browser.close") return "Closed tab";
  return tool || "Browsed";
}

function displayLabel(blk: BlockNode): string {
  const verb = (blk.attrs.action ?? "").split(".").pop() ?? "";
  const body = blk.children
    .map((c) => c.value)
    .join("")
    .replace(/^browser\.\w+\s+/, "")
    .trim();
  if (verb === "open") return `Opened ${body}`;
  if (verb === "click") return `Clicked ${body}`;
  if (verb === "type") return `Typed ${body}`;
  if (verb === "read") return body ? `Read ${body}` : "Read page";
  if (verb === "scroll") return `Scrolled ${body}`;
  if (verb === "close") return "Closed tab";
  return body || "Browsed";
}

function hintOf(args: Record<string, unknown>): string {
  for (const k of ["command", "url", "query", "name", "path", "pattern"]) {
    const v = strArg(args, k);
    if (v) return short(v);
  }
  const ref = numArg(args, "ref");
  if (ref) return ref;
  return "";
}

export function actionStep(
  blk: BlockNode,
  idx: number,
  live: boolean,
  output?: string,
  code?: number,
): ToolStep {
  const tool = blk.attrs.tool ?? "";
  const args = argsOf(blk);
  if (tool.startsWith("browser.")) {
    return {
      group: "browser",
      label: live ? browserLiveLabel(tool, args) : browserDoneLabel(tool, args),
      approvalIdx: idx,
      live,
    };
  }
  if (tool === "terminal" || tool === "bash.run") {
    const command = strArg(args, "command") || "shell";
    return { group: "terminal", label: `$ ${command}`, output, code, approvalIdx: idx, live };
  }
  const hint = hintOf(args);
  return {
    group: "tool",
    label: `${live ? "Running" : "Ran"} ${tool}${hint ? ` ${hint}` : ""}`,
    approvalIdx: idx,
    live,
  };
}

export function terminalStep(blk: BlockNode): ToolStep {
  const command = blk.attrs.command ?? "shell";
  const output = blk.children.map((c) => c.value).join("");
  return {
    group: "terminal",
    label: `$ ${command}`,
    output: output.length > 0 ? output : undefined,
  };
}

export function browserDoneStep(blk: BlockNode): ToolStep {
  return { group: "browser", label: displayLabel(blk) };
}

export type ToolStep = {
  group: "browser" | "terminal" | "tool";
  label: string;
  detail?: string;
  output?: string;
  code?: number;
  approvalIdx?: number;
  live?: boolean;
};

type Props = {
  steps: ToolStep[];
  live?: boolean;
  approval?: PendingApproval | null;
  sessionId?: string;
};

function titleFor(steps: ToolStep[], live: boolean): string {
  const groups = new Set(steps.map((s) => s.group));

  if (groups.size === 1 && groups.has("browser"))
    return live ? "Using Chrome" : "Used Chrome";
  if (groups.size === 1 && groups.has("terminal"))
    return live ? "Running command" : "Ran command";
  return live ? "Working" : "Used tools";
}

function headerIcon(steps: ToolStep[]) {
  const groups = new Set(steps.map((s) => s.group));

  if (groups.size === 1 && groups.has("browser"))
    return <SiGooglechrome size={11} />;
  if (groups.size === 1 && groups.has("terminal"))
    return <FiTerminal size={11} />;
  return <FiTool size={11} />;
}

export default function ToolActivity({
  steps,
  live = false,
  approval = null,
  sessionId,
}: Props) {
  const approvalStep = approval
    ? steps.findIndex((s) => s.approvalIdx === approval.idx)
    : -1;
  const [open, setOpen] = useState(live || approvalStep !== -1);

  useEffect(() => {
    setOpen(live || approvalStep !== -1);
  }, [live, approvalStep]);

  const decide = (allow: boolean) => {
    if (sessionId !== undefined && approval) {
      sessionStore.resolveApproval(sessionId, allow);
    }
  };

  return (
    <div className="my-2 font-sans">
      <button
        type="button"
        onClick={() => setOpen((o) => !o)}
        className="flex items-center gap-2.5 text-left"
        aria-expanded={open}
      >
        <span
          className={
            "flex h-5 w-5 shrink-0 items-center justify-center rounded-md " +
            "border border-border-primary bg-bg-secondary text-text-secondary"
          }
        >
          {headerIcon(steps)}
        </span>
        <span
          className={
            "text-sm " + `${live ? "shimmer-text" : "text-text-secondary"}`
          }
        >
          {titleFor(steps, live)}
        </span>
        <FiChevronDown
          size={12}
          className={`shrink-0 text-text-secondary transition-transform ${open ? "" : "-rotate-90"}`}
        />
      </button>
      {open && (
        <div className="ml-2.5 mt-1 flex flex-col pl-4">
          {steps.map((step, i) => (
            <div key={i} className="flex flex-col gap-1 py-1">
              <span
                className={
                  "min-w-0 max-w-[440px] truncate text-sm " +
                  `${step.live ? "shimmer-text" : "text-text-secondary"}`
                }
              >
                {step.label}
              </span>
              {step.detail && (
                <span className="truncate font-mono text-xs text-text-secondary/70">
                  {step.detail}
                </span>
              )}
              {step.output !== undefined && step.output.length > 0 && (
                <pre className="max-h-[160px] overflow-y-auto whitespace-pre-wrap font-mono text-xs leading-5 text-text-secondary">
                  {step.output}
                </pre>
              )}
              {i === approvalStep && approval && (
                <div className="flex items-center gap-2">
                  <span className="text-xs text-text-secondary">
                    Allow this action?
                  </span>
                  <button
                    type="button"
                    onClick={() => decide(true)}
                    className="rounded-full bg-white px-3 py-1 text-xs font-medium text-bg-primary hover:opacity-90"
                  >
                    Run
                  </button>
                  <button
                    type="button"
                    onClick={() => decide(false)}
                    className="rounded-full border border-border-primary px-3 py-1 text-xs text-text-secondary hover:text-text-primary"
                  >
                    Deny
                  </button>
                </div>
              )}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
