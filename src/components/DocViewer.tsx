import { openPath } from "@tauri-apps/plugin-opener";
import { useEffect, useState } from "react";
import { FcDocument } from "react-icons/fc";
import { FiDownload, FiLoader, FiX } from "react-icons/fi";
import { LuFileText, LuPresentation, LuSheet } from "react-icons/lu";

import type { LibPreview } from "../lib/ipc";
import { libraryDownload, libraryPath, libraryPreview } from "../lib/ipc";
import { docViewerStore, useDocViewer } from "../stores/docViewer";
import { toast } from "../stores/toast";

function parseCsv(text: string): string[][] {
  const rows: string[][] = [];
  let row: string[] = [];
  let cur = "";
  let quoted = false;
  for (let i = 0; i < text.length; i++) {
    const c = text[i];
    if (quoted) {
      if (c === '"') {
        if (text[i + 1] === '"') {
          cur += '"';
          i++;
        } else {
          quoted = false;
        }
      } else {
        cur += c;
      }
      continue;
    }
    if (c === '"') {
      quoted = true;
      continue;
    }
    if (c === ",") {
      row.push(cur);
      cur = "";
      continue;
    }
    if (c === "\n") {
      row.push(cur);
      rows.push(row);
      row = [];
      cur = "";
      if (rows.length >= 20) break;
      continue;
    }
    if (c === "\r") continue;
    cur += c;
  }
  row.push(cur);
  rows.push(row);
  return rows.filter((r) => r.some((c) => c.trim() !== ""));
}

function kindIcon(ext: string, kind: string) {
  if (kind === "presentation") return <LuPresentation size={30} />;
  if (kind === "sheet") return <LuSheet size={30} />;
  if (ext === "pdf" || kind === "doc") return <LuFileText size={30} />;
  return <FcDocument size={30} />;
}

export default function DocViewer() {
  const { id } = useDocViewer();
  const [preview, setPreview] = useState<LibPreview | null>(null);
  const [loading, setLoading] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  const [saved, setSaved] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    setPreview(null);
    setErr(null);
    setSaved(null);
    if (id === null) return;
    let alive = true;
    setLoading(true);
    libraryPreview(id)
      .then((p) => {
        if (alive) setPreview(p);
      })
      .catch((e) => {
        if (alive) setErr(String(e));
      })
      .finally(() => {
        if (alive) setLoading(false);
      });
    return () => {
      alive = false;
    };
  }, [id]);

  useEffect(() => {
    if (id === null) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") docViewerStore.close();
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [id]);

  if (id === null) return null;

  const close = () => docViewerStore.close();

  const openExternal = async () => {
    if (id === null) return;
    setErr(null);
    try {
      const abs = await libraryPath(id);
      await openPath(abs);
    } catch (e) {
      setErr(String(e));
    }
  };

  const download = async () => {
    if (id === null || busy) return;
    setBusy(true);
    setErr(null);
    setSaved(null);
    try {
      const res = await libraryDownload(id);
      setSaved(res.dest);
      toast.success("Downloaded");
    } catch (e) {
      setErr(String(e));
      toast.error("Download failed");
    } finally {
      setBusy(false);
    }
  };

  const rows =
    preview !== null && preview.ext === "csv" && preview.text !== null
      ? parseCsv(preview.text)
      : null;

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-4"
      onClick={close}
      role="dialog"
      aria-modal="true"
    >
      <div
        className="flex max-h-[85vh] w-full max-w-2xl flex-col overflow-hidden rounded-xl border border-border-primary bg-bg-primary"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-center gap-3 border-b border-border-primary px-4 py-3">
          <span className="shrink-0 text-text-primary">
            {preview ? (
              kindIcon(preview.ext, preview.kind)
            ) : (
              <FcDocument size={30} />
            )}
          </span>
          <div className="min-w-0 flex-1 leading-tight">
            <div className="truncate text-sm font-medium text-text-primary">
              {preview?.name ?? "Document"}
            </div>
            {preview && (
              <div className="mt-0.5 text-xs text-text-secondary">
                {preview.ext.toUpperCase()} ·{" "}
                {(preview.sz / 1024).toFixed(1)} KB
              </div>
            )}
          </div>
          <button
            type="button"
            onClick={close}
            aria-label="Close viewer"
            className={
              "flex h-7 w-7 items-center justify-center rounded-md " +
              "text-text-secondary transition-colors hover:bg-bg-hover-primary " +
              "hover:text-text-primary focus:outline-none"
            }
          >
            <FiX size={14} />
          </button>
        </div>
        <div className="min-h-0 flex-1 overflow-y-auto px-4 py-3">
          {loading && (
            <span className="shimmer-text text-sm">Loading document</span>
          )}
          {!loading && err && preview === null && (
            <p className="text-sm text-red-400">{err}</p>
          )}
          {!loading && preview && rows && (
            <table className="w-full border-collapse text-xs">
              <tbody>
                {rows.map((r, i) => (
                  <tr key={i} className="border-b border-border-primary/50">
                    {r.map((c, j) => (
                      <td
                        key={j}
                        className={
                          "px-2 py-1 align-top " +
                          (i === 0
                            ? "font-medium text-text-primary"
                            : "text-text-secondary")
                        }
                      >
                        {c}
                      </td>
                    ))}
                  </tr>
                ))}
              </tbody>
            </table>
          )}
          {!loading && preview && !rows && preview.text !== null && (
            <pre className="whitespace-pre-wrap font-sans text-sm leading-6 text-text-primary">
              {preview.text}
              {preview.truncated && (
                <span className="text-text-secondary">
                  {"\n\n…preview truncated"}
                </span>
              )}
            </pre>
          )}
          {!loading && preview && preview.text === null && (
            <div className="flex flex-col items-center gap-2 py-8 text-center">
              <span className="text-text-secondary">
                {kindIcon(preview.ext, preview.kind)}
              </span>
              <p className="max-w-sm text-sm text-text-secondary">
                {preview.ext === "pdf"
                  ? "PDF preview is not available in Argus yet. Open it externally or download a copy."
                  : "Office preview is not available in Argus yet. Open it externally or download a copy."}
              </p>
            </div>
          )}
        </div>
        <div className="flex items-center gap-2 border-t border-border-primary px-4 py-2.5">
          <button
            type="button"
            onClick={download}
            disabled={busy}
            className={
              "flex h-8 items-center gap-1.5 rounded-md border border-border-primary " +
              "px-3 text-xs font-medium text-text-primary transition-colors " +
              "hover:bg-bg-hover-primary focus:outline-none disabled:opacity-50"
            }
          >
            {busy ? (
              <FiLoader size={12} className="animate-spin" />
            ) : (
              <FiDownload size={12} />
            )}
            {busy ? "Saving" : "Download"}
          </button>
          <button
            type="button"
            onClick={openExternal}
            className={
              "flex h-8 items-center rounded-md border border-border-primary px-3 " +
              "text-xs font-medium text-text-secondary transition-colors " +
              "hover:bg-bg-hover-primary hover:text-text-primary focus:outline-none"
            }
          >
            Open
          </button>
          {saved && (
            <span className="truncate text-xs text-text-secondary">
              Saved to {saved}
            </span>
          )}
          {err && preview !== null && (
            <span className="truncate text-xs text-red-400">{err}</span>
          )}
        </div>
      </div>
    </div>
  );
}
