import { useEffect, useState } from "react";
import { FiCheck, FiChevronDown, FiGlobe } from "react-icons/fi";
import { SiGooglechrome } from "react-icons/si";

import type { BlockNode } from "../../lib/agentXml";
import { sessionStore, type PendingApproval } from "../../stores/sessions";

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

function liveLabel(blk: BlockNode): string {
  if (blk.tag !== "action") return "";
  const tool = blk.attrs.tool ?? "";
  const args = argsOf(blk);
  if (tool === "browser.open") return `Opening ${strArg(args, "url")}`;
  if (tool === "browser.click") return `Clicking ${numArg(args, "ref")}`;
  if (tool === "browser.type") return `Typing ${numArg(args, "ref")}`;
  if (tool === "browser.read") return "Reading page";
  if (tool === "browser.scroll")
    return `Scrolling ${strArg(args, "direction") || "down"}`;
  if (tool === "browser.close") return "Closing tab";
  return tool || "Browsing";
}

function doneLabel(blk: BlockNode): string {
  if (blk.tag === "browser-action") {
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
  const tool = blk.attrs.tool ?? "";
  const args = argsOf(blk);
  if (tool === "browser.open") return `Opened ${strArg(args, "url")}`;
  if (tool === "browser.click") return `Clicked ${numArg(args, "ref")}`;
  if (tool === "browser.type") return `Typed ${numArg(args, "ref")}`;
  if (tool === "browser.read") return "Read page";
  if (tool === "browser.scroll")
    return `Scrolled ${strArg(args, "direction") || "down"}`;
  if (tool === "browser.close") return "Closed tab";
  return tool || "Browsed";
}

type Props = {
  blocks: BlockNode[];
  idxs: number[];
  live?: boolean;
  approval?: PendingApproval | null;
  sessionId?: string;
};

export default function BrowserGroup({
  blocks,
  idxs,
  live = false,
  approval = null,
  sessionId,
}: Props) {
  const [open, setOpen] = useState(live);

  useEffect(() => {
    setOpen(live);
  }, [live]);

  const approvalStep = approval
    ? idxs.findIndex((v) => v === approval.idx)
    : -1;

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
          <SiGooglechrome size={11} />
        </span>
        <span
          className={
            "flex items-center gap-1.5 text-sm " +
            `${live ? "shimmer-text" : "text-text-secondary"}`
          }
        >
          <FiGlobe size={11} />
          {live ? "Using Chrome" : "Used Chrome"}
        </span>
        <FiChevronDown
          size={12}
          className={`shrink-0 text-text-secondary transition-transform ${open ? "" : "-rotate-90"}`}
        />
      </button>
      {open && (
        <div className="ml-2.5 mt-1 flex flex-col border-l border-border-primary/60 pl-4">
          {blocks.map((blk, i) => {
            const itemLive = live && i === blocks.length - 1;
            return (
              <div key={i} className="flex flex-col gap-1 py-1">
                <div className="flex items-center gap-2">
                  {itemLive ? (
                    <FiGlobe
                      size={11}
                      className="shrink-0 text-text-secondary"
                    />
                  ) : (
                    <FiCheck
                      size={11}
                      className="shrink-0 text-text-secondary/60"
                    />
                  )}
                  <span
                    className={
                      "min-w-0 max-w-[440px] truncate text-sm " +
                      `${itemLive ? "shimmer-text" : "text-text-secondary"}`
                    }
                  >
                    {itemLive ? liveLabel(blk) : doneLabel(blk)}
                  </span>
                </div>
                {i === approvalStep && approval && (
                  <div className="ml-5 flex items-center gap-2">
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
            );
          })}
        </div>
      )}
    </div>
  );
}
