import { useEffect, useRef } from "react";

import AgentBubble from "./AgentBubble";
import UsageCard from "./UsageCard";
import UserBubble from "./UserBubble";
import { parseCached, type BlockNode } from "../lib/agentXml";
import {
  docBlockFor,
  eventsFor,
  eventStep,
  noteFallback,
} from "../lib/toolEvents";
import ToolActivity, {
  actionStep,
  browserDoneStep,
  formatWorked,
  sandboxStep,
  terminalStep,
  type ToolStep,
} from "./agent/ToolActivity";
import { parseDbTime } from "../lib/relativeTime";
import {
  isSubAgentRunning,
  sessionStore,
  useSessions,
  type Turn,
} from "../stores/sessions";
import {
  learningRecordCorrection,
  type Attachment,
  type MsgRow,
  type ToolEvent,
  type UsageWindow,
} from "../lib/ipc";
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

// No attachments is not a failure — rows predating the feature have none.
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

// The backend takes only these, so an unrecognised word is the bare form.
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
  // A message with event rows here was written after events landed, so its
  // steps render from rows and its text is prose.
  const eventMap = new Map<string, ToolEvent[]>();
  for (const ev of st.events[sessionId] ?? []) {
    const list = eventMap.get(ev.message_id) ?? [];
    list.push(ev);
    eventMap.set(ev.message_id, list);
  }
  // A sub-agent counts: its turn ends before its run settles, and in that gap
  // the animation stopped and a vote row landed over an unfinished answer.
  const answering = turn !== undefined && turn.err === null;
  const running = answering || isSubAgentRunning(st, sessionId);
  // Children still running under a parent that handed out the work.
  const waiting = answering
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
          // The app answered this line itself, so there is no assistant row
          // under it and nothing to retry.
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

          // A wake is stored as `system`: the app speaking, but still something
          // the user was told and has to be able to read.
          const assistants: MsgRow[] = [];

          for (const a of group.agent) {
            if (a.role !== "assistant" && a.role !== "system") {
              continue;
            }

            // Older chats hold a duplicated retry answer. Keep the last of a run
            // so `final` survives.
            const prev = assistants[assistants.length - 1];

            if (prev !== undefined && prev.content === a.content) {
              assistants[assistants.length - 1] = a;
              continue;
            }

            assistants.push(a);
          }
          const live = running && gi === groups.length - 1;
          // Narrower than `live` on purpose: a settling sub-agent's text is already
          // written, so holding a fragment back hides content nothing follows.
          const streaming = answering && gi === groups.length - 1;
          const sessionHasEvents = eventMap.size > 0;
          const buildWorkSteps = (
            msgs: MsgRow[],
            liveIds: Set<string>,
          ): ToolStep[] => {
            const steps: ToolStep[] = [];
            let stepIdx = 0;

            for (const m of msgs) {
              const isLive = liveIds.has(m.id);

              // Events own the turn: steps from rows, doc cards from the event, no
              // text parsing. Legacy rows fall through to the parser.
              // Thinking still comes from text, so it is spliced in as a
              // paragraph step even on event-owned turns.
              if (!isLive) {
                const owned = eventsFor(eventMap, m.id);

                if (owned !== null) {
                  for (const ev of owned) {
                    steps.push(eventStep(ev));
                    stepIdx++;

                    const doc = docBlockFor(ev);
                    if (doc !== null) docBlocks.push(doc);
                  }

                  try {
                    const thoughtBlocks = parseCached(m.content, {
                      final: true,
                    });

                    for (const b of thoughtBlocks) {
                      if (b.tag !== "thinking") {
                        continue;
                      }

                      const body = b.children
                        .map((c) => c.value)
                        .join("")
                        .trim();

                      if (body.length === 0) {
                        continue;
                      }

                      steps.push({
                        group: "thought",
                        label: "Thought",
                        body,
                      });
                    }
                  } catch {
                    // Never break rendering.
                  }
                  continue;
                }
              }

              let blocks;
              try {
                // Live text is still arriving, so the parser holds back an
                // unfinished tag. In a written message it is prose.
                blocks = parseCached(m.content, { final: !isLive });
              } catch {
                continue;
              }

              // Counted, not shown: feeds the delete-the-splice decision.
              if (!isLive) {
                noteFallback(
                  m.id,
                  sessionHasEvents,
                  blocks.map((b) => b.tag),
                );
              }

              for (let bi = 0; bi < blocks.length; bi++) {
                const b = blocks[bi];

                if (b.tag === "action") {
                  const idx = stepIdx++;
                  steps.push(
                    isLive
                      ? actionStep(
                          b,
                          idx,
                          true,
                          turn?.term[idx] ?? "",
                          turn?.termCode[idx],
                        )
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
                  const body = b.children
                    .map((c) => c.value)
                    .join("")
                    .trim();

                  if (body.length > 0) {
                    steps.push({
                      group: "thought",
                      label: "Thought",
                      body,
                    });
                  }

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

                  if (texts.length > 0) {
                    steps.push({
                      group: "plan",
                      label: `Plan · ${texts.length} steps`,
                      body: texts
                        .map((t, n) => `${n + 1}. ${t}`)
                        .join("\n"),
                    });
                  }
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

          // `streaming`, not `live`: a settling sub-agent has no live row.
          const workSteps = buildWorkSteps(
            last ? [...prior, last] : prior,
            streaming && last ? new Set([last.id]) : new Set<string>(),
          );
          const showSummary = !live && workSteps.length > 0;
          // Nothing in the store says the conversation is unfinished once the parent
          // handed out work. The running children are the only signal.
          const holdOpen = !live && gi === groups.length - 1 && waiting > 0;
          const allText = assistants.map((a) => a.content).join("\n\n");

          const proseParts: string[] = [];
          const cardParts: string[] = [];

          for (const m of prior) {
            let blocks;
            try {
              blocks = parseCached(m.content, { final: true });
            } catch {
              continue;
            }

            // A sub-agent card is the only record of what was spawned, so a message
            // carrying one is kept whole.
            if (
              blocks.some((b) => b.tag === "agent" || b.tag === "agent-done")
            ) {
              cardParts.push(m.content);
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
          const priorProse = [...cardParts, proseParts.join("\n\n")]
            .filter((s) => s.length > 0)
            .join("\n\n");
          const summaryText =
            priorProse.length > 0
              ? `${priorProse}\n\n${last?.content ?? ""}`
              : last?.content;

          const startMs =
            parseDbTime(prior[0]?.created_at) ??
            parseDbTime(group.usr?.created_at);
          const endMs = live
            ? Date.now()
            : (parseDbTime(last?.created_at) ?? null);
          const workLabel = formatWorked(startMs, endMs);

          return (
            <div
              key={group.usr?.id ?? `g-${gi}`}
              className="flex flex-col gap-3"
            >
              {group.usr !== null && (
                <>
                  {userLabel !== undefined && (
                    <div className="text-xs text-text-tertiary">
                      {userLabel}
                    </div>
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
                  <ToolActivity
                    label={workLabel}
                    steps={workSteps}
                    live={false}
                  />
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

                      await learningRecordCorrection(
                        sessionId,
                        last.id,
                        edited,
                      );
                      toast.success(
                        "Correction saved — Argus will learn from it",
                      );
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
                    final={!streaming}
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
