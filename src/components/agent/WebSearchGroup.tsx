import { useEffect, useState } from "react";
import { FiChevronDown, FiGlobe } from "react-icons/fi";

import type { BlockNode } from "../../lib/agentXml";

export const WEB_ACTIONS = new Set(["web.search", "web.read"]);

function targetOf(tool: string, body: string): string {
  try {
    const args = JSON.parse(body) as { query?: string; url?: string };
    return (tool === "web.search" ? args.query : args.url) ?? body;
  } catch {
    return body;
  }
}

function verbOf(tool: string, live: boolean): string {
  if (tool === "web.search") return live ? "Searching" : "Searched";
  return live ? "Reading" : "Read";
}

type Props = { blocks: BlockNode[]; live?: boolean };

export default function WebSearchGroup({ blocks, live = false }: Props) {
  const [open, setOpen] = useState(live);

  useEffect(() => {
    setOpen(live);
  }, [live]);

  return (
    <div className="my-3 flex flex-col">
      <button
        type="button"
        onClick={() => setOpen((o) => !o)}
        className="group/web flex items-center gap-3 text-left"
        aria-expanded={open}
      >
        <span
          className={
            "flex h-6 w-6 shrink-0 items-center justify-center rounded-md " +
            "border border-border-primary bg-bg-secondary text-text-secondary"
          }
        >
          <FiGlobe size={14} />
        </span>
        <span
          className={`text-sm ${live ? "shimmer-text" : "text-text-secondary"}`}
        >
          {live ? "Searching the web" : "Searched the web"}
        </span>
        <FiChevronDown
          size={14}
          className={`text-text-secondary transition-transform duration-150 ${open ? "rotate-180" : ""}`}
        />
      </button>
      {open && (
        <div className="ml-3 flex flex-col gap-1 border-l border-border-primary/60 pl-4 pt-2">
          {blocks.map((blk, idx) => {
            const tool = blk.attrs.tool ?? "";
            const body = blk.children.map((c) => c.value).join("").trim();
            const itemLive = live && idx === blocks.length - 1;

            return (
              <div
                key={`web-${idx}`}
                className="flex items-center gap-3 py-1.5 font-sans"
              >
                <span
                  className={
                    "flex h-6 w-6 shrink-0 items-center justify-center " +
                    "rounded-md border border-border-primary bg-bg-secondary " +
                    "text-text-secondary"
                  }
                >
                  <FiGlobe size={14} />
                </span>
                <span
                  className={
                    "min-w-0 max-w-[440px] truncate text-sm " +
                    `${itemLive ? "shimmer-text" : "text-text-secondary"}`
                  }
                >
                  {verbOf(tool, itemLive)} {targetOf(tool, body)}
                </span>
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}
