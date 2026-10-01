import { Fragment, useMemo, useState } from "react";
import { FaThumbsDown, FaThumbsUp } from "react-icons/fa";
import {
  FiCheck,
  FiCopy,
  FiEdit3,
  FiRefreshCcw,
  FiThumbsDown,
  FiThumbsUp,
} from "react-icons/fi";
import type { BlockNode, XmlTree } from "../lib/agentXml";
import { blockRole, parse } from "../lib/agentXml";
import { type PendingApproval } from "../stores/sessions";
import AgentCard from "./agent/AgentCard";
import AlertBanner from "./agent/AlertBanner";
import ApprovalBlock from "./agent/ApprovalBlock";
import { MemoizedMarkdown } from "./agent/MemoizedMarkdown";
import ToolActivity, {
  actionStep,
  browserDoneStep,
  sandboxStep,
  terminalStep,
  type ToolStep,
} from "./agent/ToolActivity";
import DiffBlock from "./agent/DiffBlock";
import DocumentCard from "./agent/DocumentCard";
import EmailDraftCard from "./agent/EmailDraftCard";
import FileGroup from "./agent/FileGroup";
import MemoryRefChip from "./agent/MemoryRefChip";
import PlanBlock from "./agent/PlanBlock";
import StreamingIndicator from "./agent/StreamingIndicator";
import TableBlock from "./agent/TableBlock";
import WebSearchGroup, { WEB_ACTIONS } from "./agent/WebSearchGroup";

type LiveTerm = {
  term: Record<number, string>;
  termCode: Record<number, number>;
  approval: PendingApproval | null;
  sessionId: string;
};

type Props = {
  children?: React.ReactNode;
  text?: string;
  caret?: boolean;
  vote?: string | null;
  onVote?: (next: "up" | "down" | null) => void;
  onRetry?: () => void;
  onEdit?: (edited: string) => Promise<void>;
  liveTerm?: LiveTerm;
  hideActions?: boolean;
  hideToolActivity?: boolean;
  showThinking?: boolean;
  attachments?: BlockNode[];
  onOpenAgent?: (id: string) => void;
  /// The turn is over but the work is not: a parent that fanned out has not
  /// finished saying it, so the actions row goes but the animation stays.
  waitingSubagents?: boolean;
  /// Whether `text` is finished. False while streaming, the only case where an
  /// unterminated tag is held back rather than shown.
  final?: boolean;
};

const HEADING_CLS: Record<string, string> = {
  h1: "text-[16px] font-semibold",
  h2: "text-[16px] font-semibold",
  h3: "text-[16px] font-semibold",
  h4: "text-[16px] font-semibold",
};

function diffStats(blk: BlockNode): { added: number; removed: number } {
  const raw = blk.children.map((c) => c.value).join("");
  let added = 0;
  let removed = 0;

  for (const ln of raw.split("\n")) {
    if (ln.startsWith("+")) {
      added++;
      continue;
    }

    if (ln.startsWith("-")) {
      removed++;
    }
  }

  return { added, removed };
}

type LiveAction = {
  idx: number;
  output: string;
  code: number | undefined;
  approval: PendingApproval | null;
  sessionId: string;
};

function renderBlk(
  blk: BlockNode,
  key: string,
  onOpenAgent?: (id: string) => void,
): React.ReactNode {
  if (blk.kind === "paragraph") {
    const raw = blk.children.map((c) => c.value).join("");
    const isBulleted = /^\s*[-•*]\s+/m.test(raw);
    const isNumbered = /^\s*\d+\.\s+/m.test(raw);

    if (isBulleted || isNumbered) {
      const items = raw
        .split("\n")
        .map((l) => l.trim())
        .filter((l) => /^(?:[-•*]|\d+\.)\s+/.test(l))
        .map((l) => l.replace(/^(?:[-•*]|\d+\.)\s+/, ""));

      if (items.length === 0) {
        return (
          <div
            key={key}
            className="mt-2 font-sans text-[16px] font-medium leading-6 first:mt-0"
          >
            <MemoizedMarkdown content={raw} id={key} />
          </div>
        );
      }

      if (isNumbered) {
        return (
          <ol
            key={key}
            className="mt-2 ml-4 flex list-decimal flex-col gap-1 marker:text-text-tertiary first:mt-0"
          >
            {items.map((it, idx) => (
              <li
                key={idx}
                className="font-sans text-[16px] font-medium leading-6 text-text-primary"
              >
                <MemoizedMarkdown content={it} id={`${key}-li-${idx}`} />
              </li>
            ))}
          </ol>
        );
      }

      return (
        <ul
          key={key}
          className="mt-2 ml-4 flex list-disc flex-col gap-1 marker:text-text-tertiary first:mt-0"
        >
          {items.map((it, idx) => (
            <li
              key={idx}
              className="font-sans text-[16px] font-medium leading-6 text-text-primary"
            >
              <MemoizedMarkdown content={it} id={`${key}-li-${idx}`} />
            </li>
          ))}
        </ul>
      );
    }

    return (
      <div
        key={key}
        className="mt-2 font-sans text-[16px] font-medium leading-6 first:mt-0"
      >
        <MemoizedMarkdown content={raw} id={key} />
      </div>
    );
  }

  if (blk.kind === "heading") {
    const cls = HEADING_CLS[blk.tag] ?? "text-[16px] font-semibold";
    const raw = blk.children.map((c) => c.value).join("");

    return (
      <div
        key={key}
        className={`mt-3 font-sans leading-tight ${cls} first:mt-0`}
      >
        <MemoizedMarkdown content={raw} id={key} />
      </div>
    );
  }

  switch (blk.tag) {
    case "agent":
      return (
        <AgentCard
          key={key}
          id={blk.attrs.id ?? ""}
          name={blk.attrs.name ?? "sub-agent"}
          state={blk.attrs.state ?? "running"}
          onOpen={(id) => onOpenAgent?.(id)}
        />
      );
    case "agent-done":
      // Title only: the card is a way in, the report belongs in the sub-agent's
      // own chat. Dumping children here rendered the report as raw text.
      return (
        <AgentCard
          key={key}
          id={blk.attrs.id ?? ""}
          name={blk.attrs.name ?? "sub-agent"}
          state={blk.attrs.state ?? "done"}
          onOpen={(id) => onOpenAgent?.(id)}
        />
      );
    case "thinking":
      // Owned by the Worked summary, never the main answer.
      return null;
    case "plan":
      return <PlanBlock key={key} block={blk} />;
    case "approval":
      return <ApprovalBlock key={key} block={blk} />;
    case "diff":
      return <DiffBlock key={key} block={blk} />;
    case "document":
      return <DocumentCard key={key} block={blk} />;
    case "codeblock": {
      const code = blk.children.map((c) => c.value).join("");
      const lang = blk.attrs.language ?? "";
      return (
        <div
          key={key}
          className="mt-2 overflow-hidden rounded-lg border border-border-primary first:mt-0"
        >
          {lang !== "" && (
            <div className="border-b border-border-primary px-3 py-1 font-mono text-[10px] text-text-secondary">
              {lang}
            </div>
          )}
          <pre className="overflow-x-auto px-3 py-2 font-mono text-xs leading-5 text-text-primary">
            {code}
          </pre>
        </div>
      );
    }
    case "email-draft":
      return (
        <EmailDraftCard
          key={key}
          to={blk.attrs.to ?? ""}
          subject={blk.attrs.subject ?? ""}
          body={blk.children
            .map((c) => c.value)
            .join("")
            .trim()}
        />
      );
    case "memory-ref":
      return <MemoryRefChip key={key} block={blk} />;
    case "table":
      return <TableBlock key={key} block={blk} />;
    case "warning":
    case "error":
      return <AlertBanner key={key} block={blk} />;
    default:
      // `<agent-done>` carries a sub-agent's whole report: dropping it loses the
      // answer the parent was given.
      return (
        <div key={key} className="mt-2 flex flex-col gap-2 first:mt-0">
          {blk.children.map((c, ci) => (
            <div key={ci} className="font-sans text-[16px] font-medium leading-6">
              <MemoizedMarkdown content={c.value} id={`${key}-c-${ci}`} />
            </div>
          ))}
        </div>
      );
  }
}

function renderTree(
  tree: XmlTree,
  live: boolean,
  liveTerm?: LiveTerm,
  hideTools?: boolean,
  onOpenAgent?: (id: string) => void,
): React.ReactNode[] {
  const diffs = new Map<string, { added: number; removed: number }>();
  const paths = new Set<string>();

  for (const b of tree) {
    if (b.tag === "diff" && b.attrs.file) {
      diffs.set(b.attrs.file, diffStats(b));
    }

    if (b.tag === "file" && b.attrs.path) {
      paths.add(b.attrs.path);
    }
  }

  const out: React.ReactNode[] = [];
  let i = 0;
  let gIdx = 0;
  let pIdx = 0;
  let wIdx = 0;
  let aIdx = 0;
  let tIdx = 0;

  const actFor = (idx: number): LiveAction | undefined =>
    liveTerm === undefined
      ? undefined
      : {
          idx,
          output: liveTerm.term[idx] ?? "",
          code: liveTerm.termCode[idx],
          approval: liveTerm.approval?.idx === idx ? liveTerm.approval : null,
          sessionId: liveTerm.sessionId,
        };

  while (i < tree.length) {
    const blk = tree[i];

    // Thinking lives in the Worked summary as a paragraph, never in the
    // main answer.
    if (blk.tag === "thinking") {
      i++;
      continue;
    }

    if (hideTools === true && blockRole(blk.tag) === "work") {
      i++;
      continue;
    }

    if (blk.tag === "action" && WEB_ACTIONS.has(blk.attrs.tool ?? "")) {
      const grp: BlockNode[] = [];
      while (
        i < tree.length &&
        tree[i].tag === "action" &&
        WEB_ACTIONS.has(tree[i].attrs.tool ?? "")
      ) {
        grp.push(tree[i++]);
        aIdx++;
      }

      out.push(
        <WebSearchGroup key={`web-${wIdx++}`} blocks={grp} live={live} />,
      );
      continue;
    }

    if (blk.tag === "file") {
      const grp: BlockNode[] = [];
      while (i < tree.length && tree[i].tag === "file") {
        grp.push(tree[i++]);
      }

      const isImg = (b: BlockNode) =>
        b.attrs.type === "image" ||
        /\.(png|jpe?g|gif|webp|svg)$/i.test(b.attrs.path ?? "");
      const isDoc = (b: BlockNode) =>
        b.attrs.type === "doc" || /\.(pdf|docx?|md)$/i.test(b.attrs.path ?? "");
      const codeFiles = grp.filter((b) => !isImg(b) && !isDoc(b));
      const imgFiles = grp.filter((b) => isImg(b));

      if (codeFiles.length) {
        out.push(
          <FileGroup
            key={`file-group-${gIdx++}`}
            blocks={codeFiles}
            diffs={diffs}
          />,
        );
      }

      for (const img of imgFiles) {
        const src = img.attrs.path ?? "";
        const name = src.split("/").pop() ?? src;
        out.push(
          <img
            key={`img-${src}-${gIdx}`}
            src={src}
            alt={name}
            className="mt-3 h-auto w-full max-w-[480px] rounded-lg object-cover"
          />,
        );
      }

      continue;
    }

    if (blk.tag === "plan") {
      const steps: BlockNode[] = [];
      let j = i + 1;
      while (j < tree.length && tree[j].tag === "step") {
        steps.push(tree[j++]);
      }

      out.push(<PlanBlock key={`plan-${pIdx++}`} block={blk} steps={steps} />);
      i = j;
      continue;
    }

    if (blk.tag === "step") {
      i++;
      continue;
    }

    if (blk.tag === "diff" && blk.attrs.file && paths.has(blk.attrs.file)) {
      i++;
      continue;
    }

    if (
      blk.tag === "action" &&
      blk.attrs.tool === "filesystem.edit" &&
      paths.size > 0
    ) {
      aIdx++;
      i++;
      continue;
    }

    if (
      blk.tag === "action" ||
      blk.tag === "terminal" ||
      blk.tag === "browser-action" ||
      blk.tag === "sandbox"
    ) {
      const steps: ToolStep[] = [];

      while (
        i < tree.length &&
        (tree[i].tag === "action" ||
          tree[i].tag === "terminal" ||
          tree[i].tag === "browser-action" ||
          tree[i].tag === "sandbox")
      ) {
        const b = tree[i];

        if (b.tag === "action") {
          const act = actFor(aIdx);
          steps.push(actionStep(b, aIdx, live, act?.output, act?.code));
          aIdx++;
        } else if (b.tag === "terminal") {
          steps.push(terminalStep(b));
          aIdx++;
        } else if (b.tag === "sandbox") {
          steps.push(sandboxStep(b));
          aIdx++;
        } else {
          steps.push(browserDoneStep(b));
        }

        i++;
      }

      out.push(
        <ToolActivity
          key={`tools-${tIdx++}`}
          steps={steps}
          live={live}
          approval={liveTerm?.approval ?? null}
          sessionId={liveTerm?.sessionId}
        />,
      );
      continue;
    }

    out.push(renderBlk(blk, `b-${i}`, onOpenAgent));
    i++;
  }

  return out;
}

export default function AgentBubble({
  children,
  text,
  caret,
  vote,
  onVote,
  onRetry,
  onEdit,
  liveTerm,
  hideActions,
  hideToolActivity,
  showThinking,
  attachments,
  onOpenAgent,
  waitingSubagents,
  final = true,
}: Props) {
  // While text is arriving, a half-arrived tag is held back: rendered as raw
  // markup, a `<terminal …>` shows up in the chat as literal text.
  const tree: XmlTree | null = useMemo(
    () => (text !== undefined ? parse(text, { final }) : null),
    [text, final],
  );
  const [copied, setCopied] = useState(false);
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState("");
  const [saving, setSaving] = useState(false);
  const [editErr, setEditErr] = useState<string | null>(null);

  const copy = async () => {
    if (!text) {
      return;
    }

    try {
      await navigator.clipboard.writeText(text);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch {}
  };

  return (
    <div className="group/agent flex flex-col gap-3 text-base text-text-primary">
      {editing ? (
        <div className="flex flex-col gap-2">
          <textarea
            value={draft}
            rows={8}
            onChange={(e) => setDraft(e.target.value)}
            className={
              "w-full resize-y rounded-lg border border-border-primary " +
              "bg-bg-hover-secondary px-3 py-2 font-mono text-xs " +
              "leading-5 text-text-primary focus:outline-none " +
              "focus:ring-1 focus:ring-text-secondary"
            }
          />
          {editErr && <p className="text-xs text-red-400">{editErr}</p>}
          <div className="flex justify-end gap-1.5">
            <button
              type="button"
              onClick={() => {
                setEditing(false);
                setEditErr(null);
              }}
              className="rounded-md border border-border-primary px-2.5 py-1 text-xs text-text-secondary hover:text-text-primary cursor-pointer"
            >
              Cancel
            </button>
            <button
              type="button"
              disabled={saving || draft.trim() === ""}
              onClick={async () => {
                if (!onEdit) return;

                setSaving(true);
                setEditErr(null);

                try {
                  await onEdit(draft);
                  setEditing(false);
                } catch (e) {
                  setEditErr(String(e));
                } finally {
                  setSaving(false);
                }
              }}
              className="rounded-md bg-accent px-2.5 py-1 text-xs font-medium text-bg-primary transition-opacity hover:opacity-90 disabled:opacity-50"
            >
              {saving ? "Saving…" : "Save correction"}
            </button>
          </div>
        </div>
      ) : tree ? (
        renderTree(
          tree,
          !!caret,
          caret ? liveTerm : undefined,
          !!hideToolActivity,
          onOpenAgent,
        )
      ) : (
        <div className="font-sans text-[16px] font-light">{children}</div>
      )}

      {caret && showThinking !== false && <StreamingIndicator />}

      {!caret && waitingSubagents === true && (
        <StreamingIndicator label="Waiting for sub-agents…" />
      )}

      {attachments?.map((b, i) => (
        <Fragment key={`att-${i}`}>
          {renderBlk(b, `att-${i}`, onOpenAgent)}
        </Fragment>
      ))}

      {text !== undefined && text.length > 0 && !caret && !hideActions && (
        <div className="flex items-center gap-1 text-text-secondary">
          <button
            type="button"
            aria-label="Like response"
            onClick={() => onVote?.(vote === "up" ? null : "up")}
            className={`flex h-6 w-6 items-center justify-center rounded-md transition-colors hover:bg-bg-hover-primary hover:text-text-primary focus:outline-none focus-visible:bg-bg-hover-primary ${
              vote === "up" ? "text-text-primary" : ""
            }`}
          >
            {vote === "up" ? (
              <FaThumbsUp size={14} />
            ) : (
              <FiThumbsUp size={14} />
            )}
          </button>

          <button
            type="button"
            aria-label="Dislike response"
            onClick={() => onVote?.(vote === "down" ? null : "down")}
            className={`flex h-6 w-6 items-center justify-center rounded-md transition-colors hover:bg-bg-hover-primary hover:text-text-primary focus:outline-none focus-visible:bg-bg-hover-primary ${
              vote === "down" ? "text-text-primary" : ""
            }`}
          >
            {vote === "down" ? (
              <FaThumbsDown size={14} />
            ) : (
              <FiThumbsDown size={14} />
            )}
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

          {onEdit !== undefined && (
            <button
              type="button"
              aria-label="Edit response"
              onClick={() => {
                setDraft(text ?? "");
                setEditErr(null);
                setEditing(true);
              }}
              className="flex h-6 w-6 items-center justify-center rounded-md transition-colors hover:bg-bg-hover-primary hover:text-text-primary focus:outline-none focus-visible:bg-bg-hover-primary"
            >
              <FiEdit3 size={14} />
            </button>
          )}
        </div>
      )}
    </div>
  );
}
