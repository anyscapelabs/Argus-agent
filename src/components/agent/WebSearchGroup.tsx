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
    <div className="my-2 flex flex-col">
      <button
        type="button"
        onClick={() => setOpen((o) => !o)}
        className="group/web flex items-center gap-2.5 text-left"
        aria-expanded={open}
      >
        <span
          className={
            "flex h-5 w-5 shrink-0 items-center justify-center rounded-md " +
            "border border-border-primary bg-bg-secondary text-text-secondary"
          }
        >
          <FiGlobe size={11} />
        </span>
        <span
          className={`text-[13px] ${live ? "shimmer-text" : "text-text-secondary"}`}
        >
          {live ? "Searching the web" : "Searched the web"}
        </span>
        <FiChevronDown
          size={12}
          className={`text-text-secondary transition-transform duration-150 ${open ? "rotate-180" : ""}`}
        />
      </button>
      {open && (
        <div className="ml-2.5 flex flex-col border-l border-border-primary/60 pl-3.5">
          {blocks.map((blk, idx) => {
            const tool = blk.attrs.tool ?? "";
            const body = blk.children.map((c) => c.value).join("").trim();
            const itemLive = live && idx === blocks.length - 1;

            return (
              <div
                key={`web-${idx}`}
                className="flex items-center gap-2.5 py-1 font-sans"
              >
                <span
                  className={
                    "flex h-5 w-5 shrink-0 items-center justify-center " +
                    "rounded-md border border-border-primary bg-bg-secondary " +
                    "text-text-secondary"
                  }
                >
                  <FiGlobe size={11} />
                </span>
                <span
                  className={
                    "min-w-0 max-w-[440px] truncate text-[13px] " +
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
