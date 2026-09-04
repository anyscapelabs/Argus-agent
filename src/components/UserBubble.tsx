import { useState } from "react";
import { FiCheck, FiCopy, FiRefreshCcw } from "react-icons/fi";
import { formatRelativeTime } from "../lib/relativeTime";

type UserBubbleProps = {
  children: React.ReactNode;
  timestamp?: Date | string | number;
  onRetry?: () => void;
};

export default function UserBubble({ children, timestamp, onRetry }: UserBubbleProps) {
  const [copied, setCopied] = useState(false);

  const copy = async () => {
    const txt = typeof children === "string" ? children : "";
    if (txt.length === 0) return;
    try {
      await navigator.clipboard.writeText(txt);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch {
      // Drop it — clipboard denied
    }
  };

  return (
    <div className="group flex max-w-[70%] flex-col items-end self-end">
      <div className="rounded-2xl border border-border-primary px-4 py-2.5 font-sans text-sm font-medium text-text-primary">
        {children}
      </div>
      <div className="mt-1 flex items-center gap-1 text-text-secondary opacity-0 transition-opacity group-hover:opacity-100 group-focus-within:opacity-100">
        {timestamp !== undefined && (
          <span className="mr-1 px-1 text-[11px] font-normal">
            {formatRelativeTime(timestamp)}
          </span>
        )}
        <button
          type="button"
          aria-label="Retry message"
          onClick={onRetry}
          className="flex h-6 w-6 items-center justify-center rounded-md transition-colors hover:bg-bg-hover-primary hover:text-text-primary focus:outline-none focus-visible:bg-bg-hover-primary"
        >
          <FiRefreshCcw size={14} />
        </button>
        <button
          type="button"
          aria-label="Copy message"
          onClick={copy}
          className="flex h-6 w-6 items-center justify-center rounded-md transition-colors hover:bg-bg-hover-primary hover:text-text-primary focus:outline-none focus-visible:bg-bg-hover-primary"
        >
          {copied ? <FiCheck size={14} /> : <FiCopy size={14} />}
        </button>
      </div>
    </div>
  );
}
