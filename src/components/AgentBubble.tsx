import { Fragment, useMemo, useState } from "react";
import { FaThumbsDown, FaThumbsUp } from "react-icons/fa";
import { FiCheck, FiCopy, FiRefreshCcw, FiThumbsDown, FiThumbsUp } from "react-icons/fi";
import {
  SiGooglechrome,
  SiGmail,
  SiGoogledrive,
  SiGoogledocs,
  SiGooglesheets,
  SiGoogleslides,
  SiGooglemeet,
  SiGooglecalendar,
  SiGithub,
} from "react-icons/si";
import type { BlockNode, InlineNode, XmlTree } from "../lib/agentXml";
import ActionBlock from "./agent/ActionBlock";
import AlertBanner from "./agent/AlertBanner";
import ApprovalBlock from "./agent/ApprovalBlock";
import BrowserActionCard from "./agent/BrowserActionCard";
import DiffBlock from "./agent/DiffBlock";
import DocumentCard from "./agent/DocumentCard";
import EmailDraftCard from "./agent/EmailDraftCard";
import FileGroup from "./agent/FileGroup";
import MemoryRefChip from "./agent/MemoryRefChip";
import PlanBlock from "./agent/PlanBlock";
import TableBlock from "./agent/TableBlock";
import TerminalBlock from "./agent/TerminalBlock";
import ThinkingBlock from "./agent/ThinkingBlock";
import { parse } from "../lib/agentXml";

type Props = {
  children?: React.ReactNode;
  text?: string;
  caret?: boolean;
  onRetry?: () => void;
};

const HEADING_CLS: Record<string, string> = {
  h1: "text-[16px] font-semibold",
  h2: "text-[16px] font-semibold",
  h3: "text-[16px] font-semibold",
  h4: "text-[16px] font-semibold",
};

const INLINE_CLS: Record<string, string> = {
  bold: "font-medium",
  italic: "italic",
  underline: "underline",
  strikethrough: "line-through",
  code: "rounded bg-bg-secondary px-1 py-0.5 font-mono text-[0.9em]",
};

function renderInline(nodes: InlineNode[]): React.ReactNode {
  const out: React.ReactNode[] = [];
  let k = 0;
  type StkItem = { tag: string; href?: string };
  const stk: StkItem[] = [];

  const flush = (txt: string) => {
    if (!txt) return;
    const urlRe = /(https?:\/\/[^\s]+)/g;
    const mentionRe = /(@[a-zA-Z0-9_]+)/g;
    const toolSet = new Set(["gmail", "chrome", "drive", "docs", "sheets", "slides", "meet", "calendar", "github"]);
    const urlParts = txt.split(urlRe);
    for (const up of urlParts) {
      if (!up) continue;
      if (urlRe.test(up)) {
        urlRe.lastIndex = 0;
        out.push(<a key={`i-${k++}`} href={up} target="_blank" rel="noopener noreferrer" className="text-blue-400 underline decoration-blue-400/30 underline-offset-2 hover:text-blue-300">{up}</a>);
        continue;
      }
      const mParts = up.split(mentionRe);
      for (const mp of mParts) {
        if (!mp) continue;
        if (mentionRe.test(mp)) {
          mentionRe.lastIndex = 0;
          const tool = mp.slice(1).toLowerCase();
          if (toolSet.has(tool)) {
            const toolMeta: Record<string, { Icon: React.ComponentType<{ size?: number; className?: string }>; color: string }> = {
              chrome: { Icon: SiGooglechrome, color: "text-[#4285F4]" },
              gmail: { Icon: SiGmail, color: "text-[#EA4335]" },
              drive: { Icon: SiGoogledrive, color: "text-[#4285F4]" },
              docs: { Icon: SiGoogledocs, color: "text-[#4285F4]" },
              sheets: { Icon: SiGooglesheets, color: "text-[#0F9D58]" },
              slides: { Icon: SiGoogleslides, color: "text-[#F4B400]" },
              meet: { Icon: SiGooglemeet, color: "text-[#00897B]" },
              calendar: { Icon: SiGooglecalendar, color: "text-[#4285F4]" },
              github: { Icon: SiGithub, color: "text-text-primary" },
            };
            const meta = toolMeta[tool];
            const { Icon, color } = meta;
            out.push(
              <span key={`i-${k++}`} className="inline-flex items-center gap-1 rounded bg-bg-secondary px-1.5 py-0.5 font-sans text-xs font-medium text-text-secondary border border-border-primary">
                <Icon size={12} className={color} />
                {mp}
              </span>
            );
            continue;
          }
          out.push(<span key={`i-${k++}`} className="rounded bg-blue-500/15 px-1 py-0.5 font-medium text-blue-300">{mp}</span>);
          continue;
        }
        const cls = stk.filter((s) => s.tag !== "link").map((s) => INLINE_CLS[s.tag]).filter(Boolean).join(" ");
        const link = stk.find((s) => s.tag === "link");
        if (link?.href) {
          out.push(<a key={`i-${k++}`} href={link.href} target="_blank" rel="noopener noreferrer" className={`text-blue-400 underline decoration-blue-400/30 underline-offset-2 hover:text-blue-300 ${cls}`}>{mp}</a>);
          continue;
        }
        if (!cls) out.push(<Fragment key={`i-${k++}`}>{mp}</Fragment>);
        else out.push(<span key={`i-${k++}`} className={cls}>{mp}</span>);
      }
    }
  };

  const scan = (txt: string) => {
    const re = /<\/?([a-z][a-z0-9-]*)(?:\s+href="([^"]*)")?\s*\/?>/g;
    let cur = 0;
    let m: RegExpExecArray | null;
    let pend = "";
    while ((m = re.exec(txt)) !== null) {
      const raw = m[0];
      const name = m[1].toLowerCase();
      const href = m[2];
      const isClose = raw.startsWith("</");
      const isSelf = raw.endsWith("/>");
      const isInline = name in INLINE_CLS || name === "link";
      if (!isInline) { pend += raw; continue; }
      flush(pend + txt.slice(cur, m.index));
      pend = "";
      if (isClose) {
        const idx = stk.findLastIndex((s: StkItem) => s.tag === name);
        if (idx !== -1) stk.splice(idx, 1);
      } else if (!isSelf) {
        if (name === "link") stk.push({ tag: name, href });
        else stk.push({ tag: name });
      }
      cur = m.index + raw.length;
    }
    flush(pend + txt.slice(cur));
  };

  for (const n of nodes) scan(n.value);
  return out;
}

function diffStats(blk: BlockNode): { added: number; removed: number } {
  const raw = blk.children.map((c) => c.value).join("");
  let added = 0;
  let removed = 0;
  for (const ln of raw.split("\n")) {
    if (ln.startsWith("+")) added++;
    else if (ln.startsWith("-")) removed++;
  }
  return { added, removed };
}

function renderBlk(blk: BlockNode, key: string): React.ReactNode {
  if (blk.kind === "paragraph") {
    const raw = blk.children.map((c) => c.value).join("");
    const isBulleted = /^\s*[-•*]\s+/m.test(raw);
    const isNumbered = /^\s*\d+\.\s+/m.test(raw);
    if (isBulleted || isNumbered) {
      const items = raw.split("\n").map((l) => l.trim()).filter((l) => /^(?:[-•*]|\d+\.)\s+/.test(l)).map((l) => l.replace(/^(?:[-•*]|\d+\.)\s+/, ""));
      if (items.length > 0) {
        if (isNumbered) return <ol key={key} className="mt-2 ml-4 flex list-decimal flex-col gap-1 marker:text-text-secondary/60 first:mt-0">{items.map((it, idx) => <li key={idx} className="font-serif text-[16px] font-light leading-6 text-text-primary">{renderInline([{ kind: "text", value: it }])}</li>)}</ol>;
        return <ul key={key} className="mt-2 ml-4 flex list-disc flex-col gap-1 marker:text-text-secondary/60 first:mt-0">{items.map((it, idx) => <li key={idx} className="font-serif text-[16px] font-light leading-6 text-text-primary">{renderInline([{ kind: "text", value: it }])}</li>)}</ul>;
      }
    }
    return <p key={key} className="mt-2 font-serif text-[16px] font-light leading-6 first:mt-0">{renderInline(blk.children)}</p>;
  }
  if (blk.kind === "heading") {
    const cls = HEADING_CLS[blk.tag] ?? "text-[16px] font-semibold";
    return <div key={key} className={`mt-3 font-serif leading-tight ${cls} first:mt-0`}>{renderInline(blk.children)}</div>;
  }
  switch (blk.tag) {
    case "thinking": return <ThinkingBlock key={key} block={blk} />;
    case "plan": return <PlanBlock key={key} block={blk} />;
    case "action": return <ActionBlock key={key} block={blk} />;
    case "approval": return <ApprovalBlock key={key} block={blk} />;
    case "diff": return <DiffBlock key={key} block={blk} />;
    case "document": return <DocumentCard key={key} block={blk} />;
    case "terminal": return <TerminalBlock key={key} block={blk} />;
    case "email-draft": return <EmailDraftCard key={key} block={blk} />;
    case "browser-action": return <BrowserActionCard key={key} block={blk} />;
    case "memory-ref": return <MemoryRefChip key={key} block={blk} />;
    case "table": return <TableBlock key={key} block={blk} />;
    case "warning":
    case "error": return <AlertBanner key={key} block={blk} />;
    default: return null;
  }
}

function renderTree(tree: XmlTree): React.ReactNode[] {
  const diffs = new Map<string, { added: number; removed: number }>();
  const paths = new Set<string>();
  for (const b of tree) {
    if (b.tag === "diff" && b.attrs.file) diffs.set(b.attrs.file, diffStats(b));
    if (b.tag === "file" && b.attrs.path) paths.add(b.attrs.path);
  }
  const out: React.ReactNode[] = [];
  let i = 0;
  let gIdx = 0;
  let pIdx = 0;
  while (i < tree.length) {
    const blk = tree[i];
    if (blk.tag === "file") {
      const grp: BlockNode[] = [];
      while (i < tree.length && tree[i].tag === "file") grp.push(tree[i++]);
      const isImg = (b: BlockNode) => b.attrs.type === "image" || /\.(png|jpe?g|gif|webp|svg)$/i.test(b.attrs.path ?? "");
      const isDoc = (b: BlockNode) => b.attrs.type === "doc" || /\.(pdf|docx?|md)$/i.test(b.attrs.path ?? "");
      const codeFiles = grp.filter((b) => !isImg(b) && !isDoc(b));
      const imgFiles = grp.filter((b) => isImg(b));
      if (codeFiles.length) out.push(<FileGroup key={`file-group-${gIdx++}`} blocks={codeFiles} diffs={diffs} />);
      for (const img of imgFiles) {
        const src = img.attrs.path ?? "";
        const name = src.split("/").pop() ?? src;
        out.push(
          <img key={`img-${src}-${gIdx}`} src={src.startsWith("http") ? src : `https://picsum.photos/seed/${encodeURIComponent(src)}/800/500`} alt={name} className="mt-3 h-auto w-full max-w-[480px] rounded-lg object-cover" />,
        );
      }
      // doc files via <file> are ignored here — use <document> tag instead (per spec)
      continue;
    }
    if (blk.tag === "plan") {
      const steps: BlockNode[] = [];
      let j = i + 1;
      while (j < tree.length && tree[j].tag === "step") steps.push(tree[j++]);
      out.push(<PlanBlock key={`plan-${pIdx++}`} block={blk} steps={steps} />);
      i = j;
      continue;
    }
    if (blk.tag === "step") { i++; continue; }
    if (blk.tag === "diff" && blk.attrs.file && paths.has(blk.attrs.file)) { i++; continue; }
    if (blk.tag === "action" && blk.attrs.tool === "filesystem.edit" && paths.size > 0) { i++; continue; }
    out.push(renderBlk(blk, `b-${i}`));
    i++;
  }
  return out;
}

export default function AgentBubble({ children, text, caret, onRetry }: Props) {
  const tree: XmlTree | null = useMemo(() => (text !== undefined ? parse(text) : null), [text]);
  const [copied, setCopied] = useState(false);
  const [vote, setVote] = useState<"up" | "down" | null>(null);

  const copy = async () => {
    if (!text) return;
    try {
      await navigator.clipboard.writeText(text);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch {
      // Drop it — clipboard denied
    }
  };

  const hasBlocks = tree !== null && tree.length > 0;
  return (
    <div
      className={`group/agent flex flex-col gap-3 text-base text-text-primary ${
        caret && hasBlocks ? "stream-caret-host" : ""
      }`}
    >
      {tree ? renderTree(tree) : <div className="font-serif text-[16px] font-light">{children}</div>}
      {caret && !hasBlocks && <span className="stream-caret" />}
      {text !== undefined && text.length > 0 && (
        <div className="flex items-center gap-1 text-text-secondary opacity-0 transition-opacity group-hover/agent:opacity-100 group-focus-within/agent:opacity-100">
          <button
            type="button"
            aria-label="Like response"
            onClick={() => setVote((v) => (v === "up" ? null : "up"))}
            className={`flex h-6 w-6 items-center justify-center rounded-md transition-colors hover:bg-bg-hover-primary hover:text-text-primary focus:outline-none focus-visible:bg-bg-hover-primary ${vote === "up" ? "text-text-primary" : ""}`}
          >
            {vote === "up" ? <FaThumbsUp size={14} /> : <FiThumbsUp size={14} />}
          </button>
          <button
            type="button"
            aria-label="Dislike response"
            onClick={() => setVote((v) => (v === "down" ? null : "down"))}
            className={`flex h-6 w-6 items-center justify-center rounded-md transition-colors hover:bg-bg-hover-primary hover:text-text-primary focus:outline-none focus-visible:bg-bg-hover-primary ${vote === "down" ? "text-text-primary" : ""}`}
          >
            {vote === "down" ? <FaThumbsDown size={14} /> : <FiThumbsDown size={14} />}
          </button>
          <button
            type="button"
            aria-label="Retry response"
            onClick={onRetry}
            className="flex h-6 w-6 items-center justify-center rounded-md transition-colors hover:bg-bg-hover-primary hover:text-text-primary focus:outline-none focus-visible:bg-bg-hover-primary"
          >
            <FiRefreshCcw size={14} />
          </button>
          <button
            type="button"
            aria-label="Copy response"
            onClick={copy}
            className="flex h-6 w-6 items-center justify-center rounded-md transition-colors hover:bg-bg-hover-primary hover:text-text-primary focus:outline-none focus-visible:bg-bg-hover-primary"
          >
            {copied ? <FiCheck size={14} /> : <FiCopy size={14} />}
          </button>
        </div>
      )}
    </div>
  );
}
