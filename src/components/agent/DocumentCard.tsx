import { FcDocument } from "react-icons/fc";
import { FiDownload, FiLoader } from "react-icons/fi";

import type { BlockNode } from "../../lib/agentXml";

type Props = { block: BlockNode };

export default function DocumentCard({ block }: Props) {
  const title = block.attrs.title ?? "Untitled document";
  const doctype = block.attrs.doctype ?? "docx";
  const pages = block.attrs.pages;
  const status = block.attrs.status ?? "ready";
  const isGenerating = status === "generating";

  return (
    <div
      data-component-id={block.attrs.id}
      className="flex items-center gap-3 rounded-lg border border-border-primary bg-bg-secondary p-3 font-sans"
    >
      <FcDocument size={28} className="mt-0.5 shrink-0 self-start" />
      <div className="min-w-0 flex-1 leading-tight">
        <div className="truncate text-sm font-medium text-text-primary">
          {title}
        </div>
        <div className="mt-1 flex items-baseline gap-2 whitespace-nowrap text-xs text-text-secondary">
          <span>Document</span>
          <span aria-hidden className="text-text-secondary/60">
            ·
          </span>
          <span className="font-mono">{doctype}</span>
          {pages && (
            <>
              <span aria-hidden className="text-text-secondary/60">
                ·
              </span>
              <span>
                {pages} {pages === "1" ? "page" : "pages"}
              </span>
            </>
          )}
        </div>
      </div>
      {isGenerating ? (
        <span className="flex items-center gap-1 text-xs text-text-secondary">
          <FiLoader size={12} className="animate-spin" />
          generating
        </span>
      ) : (
        <div className="flex shrink-0 items-center gap-1">
          <button
            type="button"
            className={
              "flex h-7 items-center gap-1 rounded-md border " +
              "border-border-primary px-2 text-xs text-text-secondary " +
              "transition-colors hover:bg-bg-hover-primary " +
              "hover:text-text-primary focus:outline-none " +
              "focus-visible:bg-bg-hover-primary"
            }
          >
            Open
          </button>
          <button
            type="button"
            className={
              "flex h-7 w-7 items-center justify-center rounded-md border " +
              "border-border-primary text-text-secondary transition-colors " +
              "hover:bg-bg-hover-primary hover:text-text-primary " +
              "focus:outline-none focus-visible:bg-bg-hover-primary"
            }
            aria-label="Download"
          >
            <FiDownload size={12} />
          </button>
        </div>
      )}
    </div>
  );
}
