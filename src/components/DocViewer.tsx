import { useEffect, useRef, useState } from "react";
import { FcDocument } from "react-icons/fc";
import {
  FiChevronDown,
  FiCode,
  FiDownload,
  FiEye,
  FiMaximize2,
  FiMinimize2,
  FiPrinter,
  FiX,
} from "react-icons/fi";
import { LuFileText, LuPresentation, LuSheet } from "react-icons/lu";

import type { LibPreview } from "../lib/ipc";
import { libraryDownload, libraryPreview } from "../lib/ipc";
import { renderDocMd } from "../lib/docMd";
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

function escHtml(s: string): string {
  return s
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;");
}

export default function DocViewer() {
  const { id } = useDocViewer();
  const [preview, setPreview] = useState<LibPreview | null>(null);
  const [loading, setLoading] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  const [mode, setMode] = useState<"view" | "code">("view");
  const [width, setWidth] = useState(680);
  const [maxed, setMaxed] = useState(false);
  const [menuOpen, setMenuOpen] = useState(false);
  const [copied, setCopied] = useState(false);
  const [busy, setBusy] = useState(false);
  const drag = useRef<{ startX: number; startW: number } | null>(null);
  const prevW = useRef(680);
  const menuRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    setPreview(null);
    setErr(null);
    setMode("view");
    setMenuOpen(false);
    setCopied(false);
    if (id === null) return;
    let alive = true;
    setLoading(true);
    libraryPreview(id, 200000)
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

  useEffect(() => {
    if (!menuOpen) return;
    const onDown = (e: MouseEvent) => {
      if (menuRef.current && !menuRef.current.contains(e.target as Node)) {
        setMenuOpen(false);
      }
    };
    document.addEventListener("mousedown", onDown);
    return () => document.removeEventListener("mousedown", onDown);
  }, [menuOpen]);

  if (id === null) return null;

  const close = () => docViewerStore.close();

  const onGripDown = (e: React.MouseEvent) => {
    e.preventDefault();
    drag.current = { startX: e.clientX, startW: width };
    const move = (ev: MouseEvent) => {
      if (!drag.current) return;
      const next = drag.current.startW + (drag.current.startX - ev.clientX);
      setWidth(Math.min(960, Math.max(400, Math.round(next))));
    };
    const up = () => {
      drag.current = null;
      window.removeEventListener("mousemove", move);
      window.removeEventListener("mouseup", up);
    };
    window.addEventListener("mousemove", move);
    window.addEventListener("mouseup", up);
  };

  const copyText = async () => {
    if (preview?.text == null) return;
    try {
      await navigator.clipboard.writeText(preview.text);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch {}
  };

  const download = async () => {
    if (id === null || busy) return;
    setBusy(true);
    setMenuOpen(false);
    try {
      await libraryDownload(id);
      toast.success("Downloaded");
    } catch {
      toast.error("Download failed");
    } finally {
      setBusy(false);
    }
  };

  const toggleMax = () => {
    if (maxed) {
      setWidth(prevW.current);
      setMaxed(false);
    } else {
      prevW.current = width;
      setMaxed(true);
    }
  };

  const printPdf = () => {
    if (preview?.text == null) return;
    setMenuOpen(false);
    const frame = document.createElement("iframe");
    frame.style.position = "fixed";
    frame.style.width = "0";
    frame.style.height = "0";
    frame.style.border = "0";
    document.body.appendChild(frame);
    const doc = frame.contentDocument;
    if (!doc) {
      document.body.removeChild(frame);
      return;
    }
    doc.open();
    doc.write(
      "<!doctype html><html><head><title>" +
        escHtml(preview.name) +
        "</title><style>body{font-family:Georgia,'Times New Roman',serif;font-size:15px;line-height:1.7;color:#111;max-width:700px;margin:40px auto;padding:0 16px}pre{white-space:pre-wrap;font-family:inherit}</style></head><body><pre>" +
        escHtml(preview.text) +
        "</pre></body></html>",
    );
    doc.close();
    frame.onload = () => {
      frame.contentWindow?.print();
      setTimeout(() => document.body.removeChild(frame), 500);
    };
  };

  const rows =
    preview !== null && preview.ext === "csv" && preview.text !== null
      ? parseCsv(preview.text)
      : null;

  return (
    <div
      className="pointer-events-none fixed inset-0 z-50 flex items-stretch justify-end"
      role="dialog"
      aria-modal="false"
    >
      <div
        className="pointer-events-auto relative my-3 mr-3 flex min-h-0 w-full flex-col overflow-hidden rounded-xl border border-border-primary bg-bg-primary shadow-2xl"
        style={{
          width: maxed ? "calc(100vw - 24px)" : width,
          maxWidth: "calc(100vw - 24px)",
          height: "calc(100vh - 24px)",
        }}
        onClick={(e) => e.stopPropagation()}
      >
        {!maxed && (
          <div
            onMouseDown={onGripDown}
            aria-hidden
            className="absolute bottom-0 left-0 top-0 w-2 cursor-ew-resize"
          />
        )}
        <div className="flex items-center justify-between border-b border-border-primary px-2 py-1">
          <div
            role="tablist"
            aria-label="Viewer mode"
            className="flex items-center gap-0.5 rounded-md border border-border-primary p-0.5"
          >
            <button
              type="button"
              role="tab"
              aria-selected={mode === "view"}
              aria-label="Rendered view"
              title="Rendered view"
              onClick={() => setMode("view")}
              className={
                "flex items-center rounded px-2 py-1 " +
                "transition-colors focus:outline-none " +
                (mode === "view"
                  ? "bg-bg-hover-primary text-text-primary"
                  : "text-text-secondary hover:text-text-primary")
              }
            >
              <FiEye size={14} />
            </button>
            <button
              type="button"
              role="tab"
              aria-selected={mode === "code"}
              aria-label="Source view"
              title="Source view"
              onClick={() => setMode("code")}
              className={
                "flex items-center rounded px-2 py-1 " +
                "transition-colors focus:outline-none " +
                (mode === "code"
                  ? "bg-bg-hover-primary text-text-primary"
                  : "text-text-secondary hover:text-text-primary")
              }
            >
              <FiCode size={14} />
            </button>
          </div>
          <div className="flex items-center gap-0.5">
            <div
              ref={menuRef}
              className="relative flex items-center rounded-md border border-border-primary"
            >
              <button
                type="button"
                onClick={copyText}
                disabled={preview?.text == null}
                aria-label="Copy document"
                title="Copy document"
                className={
                  "flex items-center rounded-l-md px-2 py-1 text-xs font-medium " +
                  "text-text-secondary transition-colors hover:bg-bg-hover-primary " +
                  "hover:text-text-primary focus:outline-none disabled:opacity-40"
                }
              >
                {copied ? "Copied" : "Copy"}
              </button>
              <div aria-hidden className="h-4 w-px bg-border-primary" />
              <button
                type="button"
                onClick={() => setMenuOpen((o) => !o)}
                disabled={preview?.text == null}
                aria-label="Download options"
                title="Download options"
                aria-expanded={menuOpen}
                className={
                  "flex items-center rounded-r-md px-2 py-1 " +
                  "text-text-secondary transition-colors hover:bg-bg-hover-primary " +
                  "hover:text-text-primary focus:outline-none disabled:opacity-40"
                }
              >
                <FiChevronDown size={14} />
              </button>
              {menuOpen && (
                <div className="absolute right-0 top-8 z-10 w-44 overflow-hidden rounded-lg border border-border-primary bg-bg-primary shadow-xl">
                  <button
                    type="button"
                    onClick={download}
                    disabled={busy}
                    className={
                      "flex w-full items-center gap-2 px-3 py-2 text-left text-xs " +
                      "text-text-primary transition-colors hover:bg-bg-hover-primary " +
                      "focus:outline-none disabled:opacity-50"
                    }
                  >
                    <FiDownload size={13} />
                    {busy ? "Saving…" : "Download"}
                  </button>
                  <button
                    type="button"
                    onClick={printPdf}
                    className={
                      "flex w-full items-center gap-2 px-3 py-2 text-left text-xs " +
                      "text-text-primary transition-colors hover:bg-bg-hover-primary " +
                      "focus:outline-none"
                    }
                  >
                    <FiPrinter size={13} />
                    Print as PDF
                  </button>
                </div>
              )}
            </div>
            <button
              type="button"
              onClick={toggleMax}
              aria-label={maxed ? "Restore viewer" : "Maximize viewer"}
              title={maxed ? "Restore viewer" : "Maximize viewer"}
              className={
                "flex h-7 w-7 items-center justify-center rounded-md " +
                "text-text-secondary transition-colors hover:bg-bg-hover-primary " +
                "hover:text-text-primary focus:outline-none"
              }
            >
              {maxed ? <FiMinimize2 size={14} /> : <FiMaximize2 size={14} />}
            </button>
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
        </div>
        <div className="min-h-0 flex-1 overflow-y-auto px-4 py-3">
          <div className="mx-auto w-full max-w-[760px]">
          {loading && (
            <span className="shimmer-text text-sm">Loading document</span>
          )}
          {!loading && err && preview === null && (
            <p className="text-sm text-red-400">{err}</p>
          )}
          {!loading && preview && rows && mode === "view" && (
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
          {!loading && preview && preview.text !== null && mode === "code" && (
            <pre className="whitespace-pre-wrap font-mono text-xs leading-5 text-text-primary">
              {preview.text}
              {preview.truncated && (
                <span className="text-text-secondary">
                  {"\n\n…preview truncated"}
                </span>
              )}
            </pre>
          )}
          {!loading &&
            preview &&
            !rows &&
            preview.text !== null &&
            mode === "view" &&
            (preview.ext === "md" || preview.ext === "docx") && (
              <div className="font-serif text-[16px] font-medium leading-7 text-text-primary">
                {renderDocMd(preview.text)}
                {preview.truncated && (
                  <p className="mt-2 text-sm text-text-secondary">
                    …preview truncated
                  </p>
                )}
              </div>
            )}
          {!loading &&
            preview &&
            !rows &&
            preview.text !== null &&
            mode === "view" &&
            preview.ext !== "md" &&
            preview.ext !== "docx" && (
              <pre className="whitespace-pre-wrap font-serif text-[16px] font-medium leading-7 text-text-primary">
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
                  ? "PDF preview is not available in Argus yet."
                  : "Office preview is not available in Argus yet."}
              </p>
            </div>
          )}
          </div>
        </div>
      </div>
    </div>
  );
}
