import { useState } from "react";
import { FcDocument } from "react-icons/fc";
import { FiDownload, FiLoader } from "react-icons/fi";

import type { BlockNode } from "../../lib/agentXml";
import { libraryDownload } from "../../lib/ipc";
import { docViewerStore } from "../../stores/docViewer";
import { toast } from "../../stores/toast";

type Props = { block: BlockNode };

export default function DocumentCard({ block }: Props) {
  const title = block.attrs.title ?? "Untitled document";
  const doctype = block.attrs.doctype ?? "docx";
  const pages = block.attrs.pages;
  const status = block.attrs.status ?? "ready";
  const isGenerating = status === "generating";
  const libId = block.attrs.id ?? "";
  const [busy, setBusy] = useState(false);
  const [saved, setSaved] = useState<string | null>(null);
  const [failed, setFailed] = useState(false);

  const open = () => {
    if (!libId) return;
    docViewerStore.open(libId);
  };

  const download = async () => {
    if (!libId || busy) return;
    setBusy(true);
    setSaved(null);
    setFailed(false);
    try {
      const res = await libraryDownload(libId);
      setSaved(res.dest);
      toast.success("Downloaded");
    } catch {
      setFailed(true);
      toast.error("Download failed");
    } finally {
      setBusy(false);
    }
  };

  return (
    <div
      data-component-id={block.attrs.id}
      className="flex flex-col gap-1 rounded-lg border border-border-primary bg-bg-secondary p-3 font-sans"
    >
      <div className="flex items-center gap-3">
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
            onClick={open}
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
            onClick={download}
            disabled={busy}
            className={
              "flex h-7 w-7 items-center justify-center rounded-md border " +
              "border-border-primary text-text-secondary transition-colors " +
              "hover:bg-bg-hover-primary hover:text-text-primary " +
              "focus:outline-none focus-visible:bg-bg-hover-primary " +
              "disabled:opacity-50"
            }
            aria-label="Download"
          >
            {busy ? (
              <FiLoader size={12} className="animate-spin" />
            ) : (
              <FiDownload size={12} />
            )}
          </button>
        </div>
      )}
      </div>
      {saved && (
        <div className="truncate text-xs text-text-secondary">
          Saved to {saved}
        </div>
      )}
      {failed && (
        <div className="text-xs text-red-400">Download failed</div>
      )}
    </div>
  );
}
