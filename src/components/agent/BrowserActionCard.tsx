import { FiExternalLink, FiGlobe } from "react-icons/fi";

import type { BlockNode } from "../../lib/agentXml";

type Props = { block: BlockNode };

export default function BrowserActionCard({ block }: Props) {
  const url = block.attrs.url ?? "";
  const body = block.children.map((child) => child.value).join("").trim();

  return (
    <div className="flex items-center gap-3 rounded-lg border border-border-primary bg-bg-secondary p-3 font-sans">
      <FiGlobe size={20} className="shrink-0 text-text-primary" />
      <div className="min-w-0 flex-1">
        {body && <div className="text-sm text-text-primary">{body}</div>}
        {url && (
          <div className="mt-0.5 truncate text-xs text-text-secondary">
            {url}
          </div>
        )}
      </div>
      {url && (
        <a
          href={url}
          target="_blank"
          rel="noreferrer"
          aria-label="Open link"
          className={
            "flex h-7 w-7 shrink-0 items-center justify-center rounded-md " +
            "text-text-secondary transition-colors " +
            "hover:bg-bg-hover-primary hover:text-text-primary " +
            "focus:outline-none focus-visible:bg-bg-hover-primary"
          }
        >
          <FiExternalLink size={14} />
        </a>
      )}
    </div>
  );
}
