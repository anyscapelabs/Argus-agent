import { useState } from "react";
import { FiCheck, FiCopy, FiFile, FiRefreshCcw } from "react-icons/fi";

import { useFileSrc } from "../hooks/useLibSrc";
import { formatRelativeTime } from "../lib/relativeTime";
import type { Attachment } from "../lib/ipc";

type UserBubbleProps = {
  children: React.ReactNode;
  timestamp?: Date | string | number;
  onRetry?: () => void;
  files?: Attachment[];
};

const COPY_RESET_MS = 1500;
const EMPTY_TXT = "";

function SentFile({ file }: { file: Attachment }) {
  const src = useFileSrc(file);

  return (
    <div
      className="flex max-w-[180px] items-center gap-1.5 rounded-lg border border-border-primary bg-bg-tertiary py-1 pl-1 pr-2"
      title={file.name}
    >
      {src !== null ? (
        <img src={src} alt="" className="h-7 w-7 shrink-0 rounded object-cover" />
      ) : (
        <span className="flex h-7 w-7 shrink-0 items-center justify-center rounded bg-bg-hover-secondary">
          <FiFile size={14} className="text-text-secondary" />
        </span>
      )}

      <span className="truncate text-xs font-medium text-text-primary">
        {file.name}
      </span>
    </div>
  );
}

export default function UserBubble({
  children,
  timestamp,
  onRetry,
  files = [],
}: UserBubbleProps) {
  const [copied, setCopied] = useState(false);

  const copy = async () => {
    const txt = typeof children === "string" ? children : EMPTY_TXT;
    if (txt.length === 0) return;

    try {
      await navigator.clipboard.writeText(txt);
    } catch {
      try {
        const area = document.createElement("textarea");
        area.value = txt;
        area.style.position = "fixed";
        area.style.opacity = "0";
        document.body.appendChild(area);
        area.select();
        document.execCommand("copy");
        document.body.removeChild(area);
      } catch {
        return;
      }
    }

    setCopied(true);
    setTimeout(() => setCopied(false), COPY_RESET_MS);
  };

  return (
    <div className="group flex max-w-[70%] flex-col items-end self-end">
      {files.length > 0 && (
        <div className="mb-1 flex max-w-full flex-wrap justify-end gap-1.5">
          {files.map((file) => (
            <SentFile key={file.id} file={file} />
          ))}
        </div>
      )}

      <div
        className={
          "rounded-2xl border border-border-primary px-4 py-2.5 " +
          "font-sans text-sm font-medium text-text-primary"
        }
      >
        {children}
      </div>
      <div
        className={
          "mt-1 flex items-center gap-1 text-text-secondary opacity-0 " +
          "transition-opacity group-hover:opacity-100 " +
          "group-focus-within:opacity-100"
        }
      >
        {timestamp !== undefined && (
          <span
            className="mr-1 px-1 text-[11px] font-normal"
            title={
              typeof timestamp === "number"
                ? new Date(timestamp).toLocaleString()
                : undefined
            }
          >
            {formatRelativeTime(timestamp)}
          </span>
        )}
        <button
          type="button"
          aria-label="Retry message"
          onClick={onRetry}
          className={
            "flex h-6 w-6 items-center justify-center rounded-md " +
            "transition-colors hover:bg-bg-hover-primary " +
            "hover:text-text-primary focus:outline-none " +
            "focus-visible:bg-bg-hover-primary"
          }
        >
          <FiRefreshCcw size={14} />
        </button>
        <button
          type="button"
          aria-label="Copy message"
          onClick={copy}
          className={
            "flex h-6 w-6 items-center justify-center rounded-md " +
            "transition-colors hover:bg-bg-hover-primary " +
            "hover:text-text-primary focus:outline-none " +
            "focus-visible:bg-bg-hover-primary"
          }
        >
          {copied ? <FiCheck size={14} /> : <FiCopy size={14} />}
        </button>
      </div>
    </div>
  );
}
