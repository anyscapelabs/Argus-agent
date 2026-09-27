import { useEffect, useRef } from "react";

import AgentBubble from "./AgentBubble";
import UsageCard from "./UsageCard";
import UserBubble from "./UserBubble";
import { parseCached, type BlockNode } from "../lib/agentXml";
import ToolActivity, {
  actionStep,
  browserDoneStep,
  formatWorked,
  sandboxStep,
  terminalStep,
  type ToolStep,
} from "./agent/ToolActivity";
import { parseDbTime } from "../lib/relativeTime";
import { sessionStore, useSessions, type Turn } from "../stores/sessions";
import { learningRecordCorrection, type Attachment, type MsgRow, type UsageWindow } from "../lib/ipc";
import { toast } from "../stores/toast";

const SCROLL_LINE = 40;
const SCROLL_PAGE_RATIO = 0.85;

type Props = {
  sessionId: string;
  onOpenAgent?: (id: string) => void;
  readOnly?: boolean;
  userLabel?: string;
};

type TurnGroup = { usr: MsgRow | null; agent: MsgRow[] };

// Stored as a JSON string, because a TEXT column cannot hold a list. A row
// written before attachments existed has nothing, which is not a failure. A
// file is identified by its path now, and by its library id before that, so
// either one makes it a real file.
function parseFiles(row: MsgRow | null): Attachment[] {
  const raw = row?.attachments;
  if (raw === null || raw === undefined) {
    return [];
  }

  try {
    const found: unknown = JSON.parse(raw);
    if (!Array.isArray(found)) {
      return [];
    }

    return found.filter(
      (it): it is Attachment =>
        typeof it === "object" &&
        it !== null &&
        typeof (it as Attachment).name === "string" &&
        ((it as Attachment).path !== undefined ||
          (it as Attachment).id !== undefined),
    );
  } catch {
    return [];
  }
}

// The window a stored `/usage` was asked for, so reopening a chat lands on the
// tab that was chosen rather than on whatever the default is now. The backend
// refuses anything else, so an unrecognised word can only mean the bare form.
function localWindow(content: string): UsageWindow {
  const arg = /^\/usage\s+(\S+)/i.exec(content)?.[1] ?? "";

  return arg === "today" || arg === "month" ? arg : "week";
}

function groupTurns(
  rows: MsgRow[],
  turn: Turn | undefined,
  sessionId: string,
): TurnGroup[] {
  const groups: TurnGroup[] = [];

  for (const r of rows) {
    if (r.role === "user" && !r.content.startsWith("<tool-result")) {
      groups.push({ usr: r, agent: [] });
      continue;
    }

    if (groups.length === 0) {
      groups.push({ usr: null, agent: [] });
    }

    groups[groups.length - 1].agent.push(r);
  }

  if (turn === undefined || turn.err !== null) {
    return groups;
  }

  if (groups.length === 0) {
    groups.push({ usr: null, agent: [] });
  }

  groups[groups.length - 1].agent.push({
    id: "live",
    session_id: sessionId,
    seq: 0,
    role: "assistant",
    content: turn.text,
    model_id: null,
    provider_id: null,
    tok_in: null,
    tok_out: null,
    active: true,
    vote: null,
    created_at: "",
  });

  return groups;
}

export default function ChatTranscript({
  sessionId,
  onOpenAgent,
  readOnly = false,
  userLabel,
}: Props) {
  const st = useSessions();
  const notes = st.notes[sessionId] ?? [];
  const { msgs, turns, stopped } = st;
  const scrollRef = useRef<HTMLDivElement | null>(null);
  const pinnedRef = useRef(true);
  const rafRef = useRef(0);

  const rows = msgs[sessionId] ?? [];
  const turn = turns[sessionId];
  const running = turn !== undefined && turn.err === null;
  // The turn can be over while the work is not. A parent that fanned out and
  // said it would report when the children finish has not finished saying it.
  const waiting = running
    ? 0
    : (st.sessions.find((s) => s.id === sessionId)?.running_agents ?? 0);
  const wasStopped = stopped[sessionId] === true && !running;

  const groups = groupTurns(rows, turn, sessionId);

  const voteOf = (msgId: string) =>
    rows.find((m) => m.id === msgId)?.vote ?? null;

  useEffect(() => {
    const onScroll = () => {
      const el = scrollRef.current;
      if (!el) {
        return;
      }

      const nearBottom = el.scrollHeight - el.scrollTop - el.clientHeight < 80;
      pinnedRef.current = nearBottom;
    };

    const el = scrollRef.current;
    if (!el) {
      return;
    }

    el.addEventListener("scroll", onScroll, { passive: true });

    return () => el.removeEventListener("scroll", onScroll);
  }, []);

  useEffect(() => {
    pinnedRef.current = true;
    const el = scrollRef.current;
    if (!el) {
      return;
    }

    el.scrollTop = el.scrollHeight;
  }, [sessionId]);

  useEffect(() => {
    if (!pinnedRef.current) {
      return;
    }

    cancelAnimationFrame(rafRef.current);
    rafRef.current = requestAnimationFrame(() => {
      const el = scrollRef.current;
      if (el === null || !pinnedRef.current) {
        return;
      }

      el.scrollTop = el.scrollHeight;
    });

    return () => cancelAnimationFrame(rafRef.current);
  }, [rows.length, turn?.text]);

  const retryFrom = (usrMsgId: string) => {
    if (running || readOnly) {
      return;
    }

    const row = rows.find((m) => m.id === usrMsgId && m.role === "user");
    if (row === undefined) {
      return;
    }

    sessionStore.retry(sessionId, row.seq, row.content, parseFiles(row));
  };

  const handleScrollKey = (event: React.KeyboardEvent<HTMLDivElement>) => {
    const el = scrollRef.current;
    if (!el) {
      return;
    }

    const pageAmount = el.clientHeight * SCROLL_PAGE_RATIO;

    switch (event.key) {
      case "PageDown":
        el.scrollBy({ top: pageAmount, behavior: "smooth" });
        event.preventDefault();
        break;
      case "PageUp":
        el.scrollBy({ top: -pageAmount, behavior: "smooth" });
        event.preventDefault();
        break;
      case "ArrowDown":
        el.scrollBy({ top: SCROLL_LINE, behavior: "smooth" });
        event.preventDefault();
        break;
      case "ArrowUp":
        el.scrollBy({ top: -SCROLL_LINE, behavior: "smooth" });
        event.preventDefault();
        break;
      case "Home":
        el.scrollTo({ top: 0, behavior: "smooth" });
        event.preventDefault();
        break;
      case "End":
        el.scrollTo({ top: el.scrollHeight, behavior: "smooth" });
        event.preventDefault();
        break;
      default:
        break;
    }
  };

  return (
    <div
      ref={scrollRef}
      tabIndex={0}
      onKeyDown={handleScrollKey}
      className="min-h-0 flex-1 overflow-y-auto px-6 pb-16 pt-6 outline-none"
    >
      <div className="mx-auto flex w-full min-w-0 max-w-[700px] flex-col gap-3">
        {groups.map((group, gi) => {
          // A line the app answered itself. It is a real turn in the
          // transcript and the model was never asked, so there is no
          // assistant row under it, no work label, and nothing to retry —
          // just the line and what it drew.
          if (group.usr?.local === true) {
            return (
              <div key={group.usr.id} className="flex flex-col gap-3">
                <UserBubble
                  files={parseFiles(group.usr)}
                  timestamp={parseDbTime(group.usr.created_at) ?? undefined}
                >
                  {group.usr.content}
                </UserBubble>
                <UsageCard window={localWindow(group.usr.content)} />
              </div>
            );
          }

          // A wake — a sub-agent or a job reporting back — is stored as
          // `system`. It is the app speaking, not the model, but it is still
          // something the user was told and has to be able to read.
          const assistants: MsgRow[] = [];

          for (const a of group.agent) {
            if (a.role !== "assistant" && a.role !== "system") {
              continue;
            }

            // A model asked to try again and answering the same way used to
            // land as a second copy of the same message. The backend stops that
            // now; this keeps the chats that already have it from reading as
            // three answers. Keep the last of a run, so `final` survives.
            const prev = assistants[assistants.length - 1];

            if (prev !== undefined && prev.content === a.content) {
              assistants[assistants.length - 1] = a;
              continue;
            }

            assistants.push(a);
          }
          const live = running && gi === groups.length - 1;
          const buildWorkSteps = (
            msgs: MsgRow[],
            liveIds: Set<string>,
          ): ToolStep[] => {
            const steps: ToolStep[] = [];
            let stepIdx = 0;

            for (const m of msgs) {
              let blocks;
              try {
                blocks = parseCached(m.content);
              } catch {
                continue;
              }

              const isLive = liveIds.has(m.id);

              for (let bi = 0; bi < blocks.length; bi++) {
                const b = blocks[bi];

                if (b.tag === "action") {
                  const idx = stepIdx++;
                  steps.push(
                    isLive
                      ? actionStep(b, idx, true, turn?.term[idx] ?? "", turn?.termCode[idx])
                      : actionStep(b, idx, false),
                  );
                } else if (b.tag === "terminal") {
                  const s = terminalStep(b);
                  steps.push(isLive ? s : { ...s, output: undefined });
                } else if (b.tag === "sandbox") {
                  const s = sandboxStep(b);
                  steps.push(isLive ? s : { ...s, output: undefined });
                } else if (b.tag === "browser-action") {
                  steps.push(browserDoneStep(b));
                } else if (b.tag === "document") {
                  steps.push({
                    group: "tool",
                    label: `Created document ${b.attrs.title ?? b.attrs.name ?? b.attrs.id ?? "document"}`,
                  });
                  docBlocks.push(b);
                } else if (b.tag === "check") {
                  steps.push({
                    group: "tool",
                    label:
                      b.attrs.status === "retry"
                        ? "Caught an issue, retrying"
                        : "Checked the answer",
                  });
                } else if (b.tag === "thinking") {
                  const texts: string[] = [];
                  let j = bi + 1;

                  while (j < blocks.length && blocks[j].tag === "step") {
                    texts.push(
                      blocks[j].children
                        .map((c) => c.value)
                        .join("")
                        .trim(),
                    );
                    j++;
                  }
                  bi = j - 1;

                  steps.push({
                    group: "plan",
                    label:
                      texts.length > 0
                        ? `Plan · ${texts.length} steps`
                        : "Plan",
                    body:
                      texts.length > 0
                        ? texts.map((t, n) => `${n + 1}. ${t}`).join("\n")
                        : undefined,
                  });
                }
              }
            }

            return steps;
          };

          const docBlocks: BlockNode[] = [];

          let last: MsgRow | undefined;
          let prior: MsgRow[];

          if (live) {
            last = assistants[assistants.length - 1];
            prior = assistants
              .slice(0, -1)
              .filter((a) => a.content.trim().length > 0);
          } else {
            last =
              assistants.find((a) => a.kind === "final") ??
              assistants[assistants.length - 1];
            prior = assistants.filter(
              (a) => a !== last && a.content.trim().length > 0,
            );
          }

          const workSteps = buildWorkSteps(
            last ? [...prior, last] : prior,
            live && last ? new Set([last.id]) : new Set<string>(),
          );
          const showSummary = !live && workSteps.length > 0;
          // The parent's turn ended when it handed the work out, so nothing in
          // the store says the conversation is unfinished — the children still
          // running are the only thing that knows. The last bubble holds the
          // thinking animation instead of a vote row for an answer the model
          // itself has not finished giving.
          const holdOpen = !live && gi === groups.length - 1 && waiting > 0;
          const allText = assistants.map((a) => a.content).join("\n\n");

          const proseParts: string[] = [];
          for (const m of prior) {
            let blocks;
            try {
              blocks = parseCached(m.content);
            } catch {
              continue;
            }
            for (const b of blocks) {
              if (b.kind !== "paragraph" && b.kind !== "heading") {
                continue;
              }
              const t = b.children.map((c) => c.value).join("");
              if (t.trim().length === 0) {
                continue;
              }
              proseParts.push(b.kind === "heading" ? `# ${t}` : t);
            }
          }
          const priorProse = proseParts.join("\n\n");
          const summaryText =
            priorProse.length > 0
              ? `${priorProse}\n\n${last?.content ?? ""}`
              : last?.content;

          const startMs =
            parseDbTime(prior[0]?.created_at) ?? parseDbTime(group.usr?.created_at);
          const endMs = live
            ? Date.now()
            : (parseDbTime(last?.created_at) ?? null);
          const workLabel = formatWorked(startMs, endMs);

          return (
            <div key={group.usr?.id ?? `g-${gi}`} className="flex flex-col gap-3">
              {group.usr !== null && (
                <>
                  {userLabel !== undefined && (
                    <div className="text-xs text-text-tertiary">{userLabel}</div>
                  )}
                  <UserBubble
                    files={parseFiles(group.usr)}
                    timestamp={parseDbTime(group.usr.created_at) ?? undefined}
                    onRetry={
                      readOnly
                        ? undefined
                        : () => {
                            if (group.usr !== null) {
                              retryFrom(group.usr.id);
                            }
                          }
                    }
                  >
                    {group.usr.content}
                  </UserBubble>
                </>
              )}
              {showSummary ? (
                <>
                  <ToolActivity label={workLabel} steps={workSteps} live={false} />
                  <AgentBubble
                    onOpenAgent={onOpenAgent}
                    text={summaryText}
                    hideToolActivity
                    attachments={docBlocks}
                    vote={voteOf(last?.id ?? "")}
                    onVote={(v) => {
                      if (last === undefined) {
                        return;
                      }

                      sessionStore.setVote(sessionId, last.id, v);
                    }}
                    hideActions={holdOpen}
                    waitingSubagents={holdOpen}
                    onEdit={async (edited) => {
                      if (last === undefined) return;

                      await learningRecordCorrection(sessionId, last.id, edited);
                      toast.success("Correction saved — Argus will learn from it");
                    }}
                    onRetry={
                      readOnly || group.usr === null
                        ? undefined
                        : () => {
                            if (group.usr !== null) {
                              retryFrom(group.usr.id);
                            }
                          }
                    }
                  />
                </>
              ) : live ? (
                <>
                  {workSteps.length > 0 && (
                    <ToolActivity
                      steps={workSteps}
                      live
                      liveStartedAt={startMs}
                      approval={turn?.approval ?? null}
                      sessionId={sessionId}
                    />
                  )}
                  <AgentBubble
                    onOpenAgent={onOpenAgent}
                    text={allText}
                    caret
                    hideToolActivity
                    showThinking={(turn?.text ?? "").length === 0}
                    liveTerm={{
                      term: turn?.term ?? {},
                      termCode: turn?.termCode ?? {},
                      approval: turn?.approval ?? null,
                      sessionId,
                    }}
                  />
                </>
              ) : (
                <AgentBubble
                  onOpenAgent={onOpenAgent}
                  text={allText}
                  caret={live}
                  liveTerm={
                    live
                      ? {
                          term: turn?.term ?? {},
                          termCode: turn?.termCode ?? {},
                          approval: turn?.approval ?? null,
                          sessionId,
                        }
                      : undefined
                  }
                  vote={live ? null : holdOpen ? null : voteOf(last?.id ?? "")}
                  hideActions={holdOpen}
                  waitingSubagents={holdOpen}
                  onVote={(v) => {
                    if (last === undefined) {
                      return;
                    }

                    sessionStore.setVote(sessionId, last.id, v);
                  }}
                  onRetry={
                    readOnly || group.usr === null
                      ? undefined
                      : () => {
                          if (group.usr !== null) {
                            retryFrom(group.usr.id);
                          }
                        }
                  }
                />
              )}
            </div>
          );
        })}
        {notes.map((note, ni) => (
          <div
            key={`note-${ni}`}
            className={
              "rounded-xl border border-border-primary bg-bg-secondary px-3 py-2 " +
              "font-mono text-xs whitespace-pre-wrap text-text-primary"
            }
          >
            {note.text}
          </div>
        ))}
        {running && turn?.status != null && (
          <div className="text-xs text-text-tertiary">
            {turn.status.attempt > 1
              ? `Retrying — ${turn.status.providerId}, attempt ${turn.status.attempt}`
              : `Asking ${turn.status.providerId}`}
          </div>
        )}
        {turn?.err !== undefined && turn !== undefined && turn.err !== null && (
          <div
            className={
              "rounded-xl border border-red-500/30 " +
              "bg-red-500/10 px-3 py-2 text-sm text-red-400"
            }
          >
            {turn.err}
          </div>
        )}
        {wasStopped && (
          <div
            className={
              "rounded-xl border border-border-primary " +
              "bg-bg-secondary px-3 py-2 text-center text-xs " +
              "text-text-secondary"
            }
          >
            Stopped — partial work above is saved.
          </div>
        )}
      </div>
    </div>
  );
}
