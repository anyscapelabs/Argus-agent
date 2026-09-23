import { useEffect, useRef, useState } from "react";
import { FiCheck, FiChevronDown, FiList, FiMessageSquare, FiShield, FiTerminal, FiTool, FiX } from "react-icons/fi";
import { SiGooglechrome } from "react-icons/si";

import { sessionStore, type PendingApproval } from "../../stores/sessions";
import { useWorkTimer } from "../../stores/workTimer";

import type { BlockNode } from "../../lib/agentXml";
import EmailDraftCard from "./EmailDraftCard";

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

  if (tool === "gmail.send" || tool === "outlook.send") {
    const to = strArg(args, "to");

    return {
      group: "tool",
      tool,
      args,
      label: live ? `Drafting email to ${to}` : `Email to ${to}`,
      approvalIdx: idx,
      live,
    };
  }

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
    const labelArg = strArg(args, "label");
    return {
      group: "terminal",
      label: labelArg || `$ ${command}`,
      detail: labelArg ? `$ ${command}` : undefined,
      output,
      code,
      approvalIdx: idx,
      live: live && code === undefined,
    };
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
  const ms = Number(blk.attrs.duration_ms ?? "");
  return {
    group: "terminal",
    label: `$ ${command}`,
    durationMs: Number.isFinite(ms) && ms > 0 ? ms : undefined,
    output: output.length > 0 ? output : undefined,
  };
}

export function fmtDuration(ms: number): string {
  if (ms >= 60_000) return `${(ms / 60_000).toFixed(1)}m`;
  if (ms >= 1_000) return `${(ms / 1_000).toFixed(1)}s`;
  return `${Math.round(ms)}ms`;
}

export function sandboxStep(blk: BlockNode): ToolStep {
  const command = blk.attrs.command ?? "command";
  const output = blk.children.map((c) => c.value).join("");
  const bits = ["sandboxed", blk.attrs.profile, blk.attrs.origin].filter(
    (b) => b !== undefined && b !== "",
  );

  return {
    group: "sandbox",
    label: `$ ${command}`,
    detail: bits.join(" · "),
    output: output.length > 0 ? output : undefined,
    badge: "sandboxed",
  };
}

export function formatDuration(ms: number): string {
  const totalSec = Math.max(0, Math.round(ms / 1000));

  if (totalSec < 60) {
    return `${totalSec} sec`;
  }

  const mins = Math.floor(totalSec / 60);
  const secs = totalSec % 60;

  if (mins < 60) {
    return secs === 0 ? `${mins} min` : `${mins} min ${secs} sec`;
  }

  const hours = Math.floor(mins / 60);
  const remMin = mins % 60;

  return remMin === 0 ? `${hours} hr` : `${hours} hr ${remMin} min`;
}

export function formatWorked(startMs: number | null, endMs: number | null): string {
  if (startMs === null || endMs === null || endMs < startMs) {
    return "Work details";
  }

  return `Worked for ${formatDuration(endMs - startMs)}`;
}

function ThoughtRow({ step }: { step: ToolStep }) {
  const [show, setShow] = useState(false);
  const Icon = step.group === "plan" ? FiList : FiMessageSquare;

  return (
    <div className="flex flex-col gap-1 px-3 py-2">
      <button
        type="button"
        onClick={() => setShow((v) => !v)}
        className="flex items-center gap-2 text-left cursor-pointer"
      >
        <Icon size={13} className="shrink-0 text-text-secondary" />
        <span className="min-w-0 flex-1 truncate text-sm text-text-secondary">
          {step.label}
        </span>
        {step.body && (
          <span className="shrink-0 text-xs text-text-secondary/70">
            {show ? "Hide ↑" : "Show →"}
          </span>
        )}
      </button>
      {show && step.body && (
        <p className="max-h-[240px] overflow-y-auto whitespace-pre-wrap pl-6 text-sm leading-6 text-text-secondary">
          {step.body}
        </p>
      )}
    </div>
  );
}

export function browserDoneStep(blk: BlockNode): ToolStep {
  return { group: "browser", label: displayLabel(blk) };
}

export type ToolStep = {
  group: "browser" | "terminal" | "tool" | "thought" | "plan" | "sandbox";
  label: string;
  detail?: string;
  output?: string;
  code?: number;
  durationMs?: number;
  body?: string;
  approvalIdx?: number;
  live?: boolean;
  tool?: string;
  args?: Record<string, unknown>;
  badge?: string;
};

type Props = {
  steps: ToolStep[];
  live?: boolean;
  approval?: PendingApproval | null;
  sessionId?: string;
  label?: string;
  liveStartedAt?: number | null;
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

function TerminalActivity({
  step,
  approval,
  onAllow,
  onDeny,
}: {
  step: ToolStep;
  approval: boolean;
  onAllow: () => void;
  onDeny: () => void;
}) {
  const [show, setShow] = useState(false);
  const denied = step.code === -2;
  const failed =
    step.code !== undefined && step.code !== 0 && !denied && !step.live;
  const done = !step.live && step.code !== undefined && step.code === 0;
  const hasDetails =
    (step.output !== undefined && step.output.length > 0) ||
    step.code !== undefined ||
    step.durationMs !== undefined;

  return (
    <div className="flex flex-col gap-1 px-3 py-2">
      <div className="flex items-center gap-2">
        {step.live ? (
          <span className="shimmer-text text-xs">◌</span>
        ) : done ? (
          <FiCheck size={13} className="shrink-0 text-green-500" />
        ) : failed || denied ? (
          <FiX
            size={13}
            className={`shrink-0 ${failed ? "text-red-400" : "text-text-secondary"}`}
          />
        ) : (
          <FiTerminal size={13} className="shrink-0 text-text-secondary" />
        )}
        <span
          className={
            "min-w-0 flex-1 truncate text-sm " +
            `${step.live ? "shimmer-text" : "text-text-primary"}`
          }
        >
          {denied ? "Denied" : step.label}
        </span>
        {step.badge && (
          <span className="flex shrink-0 items-center gap-1 rounded-md border border-border-primary px-1.5 py-0.5 text-[10px] text-text-secondary">
            <FiShield size={10} />
            {step.badge}
          </span>
        )}
        {hasDetails && !approval && (
          <button
            type="button"
            onClick={() => setShow((v) => !v)}
            className="shrink-0 text-xs text-text-secondary hover:text-text-primary cursor-pointer"
          >
            {show ? "Hide details ↑" : "Show details →"}
          </button>
        )}
      </div>
      {step.detail && (
        <span className="truncate font-mono text-xs text-text-secondary/70">
          {step.detail}
        </span>
      )}
      {approval && (
        <div className="flex items-center gap-2">
          <span className="text-xs text-text-secondary">
            Allow this action?
          </span>
          <button
            type="button"
            onClick={onAllow}
            className="rounded-md bg-white px-3 py-1 text-xs font-medium text-bg-primary hover:opacity-90 cursor-pointer"
          >
            Run
          </button>
          <button
            type="button"
            onClick={onDeny}
            className="rounded-md border border-border-primary px-3 py-1 text-xs text-text-secondary hover:text-text-primary cursor-pointer"
          >
            Deny
          </button>
        </div>
      )}
      {show && (
        <div className="flex flex-col gap-1.5 pt-1">
          {(step.code !== undefined || step.durationMs !== undefined) && (
            <span className="font-mono text-xs text-text-secondary">
              {step.code !== undefined ? `exit ${step.code}` : ""}
              {step.code !== undefined && step.durationMs !== undefined
                ? " · "
                : ""}
              {step.durationMs !== undefined
                ? fmtDuration(step.durationMs)
                : ""}
            </span>
          )}
          {step.output !== undefined && step.output.length > 0 && (
            <pre className="max-h-[180px] overflow-y-auto whitespace-pre-wrap font-mono text-sm leading-6 text-text-primary">
              {step.output}
            </pre>
          )}
        </div>
      )}
    </div>
  );
}

export default function ToolActivity({
  steps,
  live = false,
  approval = null,
  sessionId,
  label,
  liveStartedAt = null,
}: Props) {
  const approvalStep = approval
    ? steps.findIndex((s) => s.approvalIdx === approval.idx)
    : -1;
  const [open, setOpen] = useState(live || approvalStep !== -1);
  const now = useWorkTimer((s) => s.now);
  const mountedAt = useRef(Date.now());

  useEffect(() => {
    setOpen(live || approvalStep !== -1);
  }, [live, approvalStep]);

  useEffect(() => {
    if (!live) {
      return;
    }

    useWorkTimer.getState().start();

    return () => {
      useWorkTimer.getState().stop();
    };
  }, [live]);

  const header =
    label ??
    (live
      ? `Working… ${formatDuration(now - (liveStartedAt ?? mountedAt.current))}`
      : titleFor(steps, live));

  const decide = (allow: boolean) => {
    if (sessionId !== undefined && approval) {
      sessionStore.resolveApproval(sessionId, allow);
    }
  };

  const isEmailStep = (step: ToolStep) =>
    step.tool === "gmail.send" || step.tool === "outlook.send";

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
          {header}
        </span>
        <FiChevronDown
          size={12}
          className={`shrink-0 text-text-secondary transition-transform ${open ? "" : "-rotate-90"}`}
        />
      </button>
      {open && (
        <div className="mt-1.5 overflow-hidden rounded-lg border border-border-primary">
          <div className="flex flex-col divide-y divide-border-primary">
          {steps.map((step, i) =>
            step.group === "thought" || step.group === "plan" ? (
              <ThoughtRow key={i} step={step} />
            ) : step.group === "terminal" || step.group === "sandbox" ? (
                <TerminalActivity
                  key={i}
                  step={step}
                  approval={i === approvalStep && approval !== null}
                  onAllow={() => decide(true)}
                  onDeny={() => decide(false)}
                />
              ) : (
                <div key={i} className="flex flex-col gap-1 px-3 py-2">
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
              {i === approvalStep && approval && isEmailStep(step) && (
                <EmailDraftCard
                  to={strArg(step.args ?? {}, "to")}
                  subject={strArg(step.args ?? {}, "subject")}
                  body={strArg(step.args ?? {}, "body")}
                  onSend={(d) => {
                    if (sessionId === undefined) return;

                    sessionStore.resolveApproval(
                      sessionId,
                      true,
                      JSON.stringify({ ...(step.args ?? {}), ...d }),
                    );
                  }}
                  onDiscard={() => decide(false)}
                />
              )}
              {i === approvalStep && approval && !isEmailStep(step) && (
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
          )
        )}
        </div>
      </div>
      )}
    </div>
  );
}
