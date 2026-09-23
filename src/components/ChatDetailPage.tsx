import { useEffect, useRef, useState } from "react";

import AgentBubble from "./AgentBubble";
import ChatInput from "./ChatInput";
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
import { useChatModels } from "../hooks/useChatModels";
import { sessionStore, useSessions, type Turn } from "../stores/sessions";
import type { MsgRow } from "../lib/ipc";
import { learningRecordCorrection } from "../lib/ipc";
import { toast } from "../stores/toast";

const SCROLL_LINE = 40;
const SCROLL_PAGE_RATIO = 0.85;

type Props = { sessionId: string };

type TurnGroup = { usr: MsgRow | null; agent: MsgRow[] };

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

export default function ChatDetailPage({ sessionId }: Props) {
  const { sessions, msgs, turns, stopped } = useSessions();
  const scrollRef = useRef<HTMLDivElement | null>(null);
  const pinnedRef = useRef(true);
  const rafRef = useRef(0);
  const [draft, setDraft] = useState("");
  const rows = msgs[sessionId] ?? [];
  const turn = turns[sessionId];
  const running = turn !== undefined && turn.err === null;
  const wasStopped = stopped[sessionId] === true && !running;

  const { models } = useChatModels();
  const session = sessions.find((s) => s.id === sessionId);
  const model =
    models.find((m) => m.modelId === session?.model_id) ?? null;

  const groups = groupTurns(rows, turn, sessionId);

  const voteOf = (msgId: string) =>
    rows.find((m) => m.id === msgId)?.vote ?? null;

  useEffect(() => {
    const onScroll = () => {
      const el = scrollRef.current;
      if (!el) {
        return;
      }

      const nearBottom =
        el.scrollHeight - el.scrollTop - el.clientHeight < 80;
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

  const handleSend = (text: string) => {
    if (running) {
      return;
    }

    sessionStore.send(sessionId, text);
  };

  const retryFrom = (usrMsgId: string) => {
    if (running) {
      return;
    }

    const row = rows.find(
      (m) => m.id === usrMsgId && m.role === "user",
    );
    if (row === undefined) {
      return;
    }

    sessionStore.retry(sessionId, row.seq, row.content);
  };

  const handleScrollKey = (
    event: React.KeyboardEvent<HTMLDivElement>,
  ) => {
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
    <div className="flex h-full min-h-0 w-full min-w-0 flex-col">
      <div
        ref={scrollRef}
        tabIndex={0}
        onKeyDown={handleScrollKey}
        className="min-h-0 flex-1 overflow-y-auto px-6 pb-16 pt-6 outline-none"
      >
        <div
          className="mx-auto flex w-full min-w-0 max-w-[700px] flex-col gap-3"
        >
          {groups.map((group, gi) => {
            const assistants = group.agent.filter((a) => a.role === "assistant");
            const live = running && gi === groups.length - 1;
            const buildWorkSteps = (msgs: MsgRow[], liveIds: Set<string>): ToolStep[] => {
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
                          ? texts
                              .map((t, n) => `${n + 1}. ${t}`)
                              .join("\n")
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
                  <UserBubble
                    timestamp={parseDbTime(group.usr.created_at) ?? undefined}
                    onRetry={() => {
                      if (group.usr !== null) {
                        retryFrom(group.usr.id);
                      }
                    }}
                  >
                    {group.usr.content}
                  </UserBubble>
                )}
                {showSummary ? (
                  <>
                    <ToolActivity
                      label={workLabel}
                      steps={workSteps}
                      live={false}
                    />
                    <AgentBubble
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
                      onRetry={() => {
                        if (group.usr === null) {
                          return;
                        }

                        retryFrom(group.usr.id);
                      }}
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
                    vote={live ? null : voteOf(last?.id ?? "")}
                    onVote={(v) => {
                      if (last === undefined) {
                        return;
                      }

                      sessionStore.setVote(sessionId, last.id, v);
                    }}
                    onRetry={() => {
                      if (group.usr === null) {
                        return;
                      }

                      retryFrom(group.usr.id);
                    }}
                  />
                )}
              </div>
            );
          })}
          {turn?.err !== undefined &&
            turn !== undefined &&
            turn.err !== null && (
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

      <div className="flex w-full shrink-0 justify-center pb-3 pt-2">
        <ChatInput
          placeholder="Reply to Argus…"
          value={draft}
          onChange={setDraft}
          model={model}
          onModelChange={(m) =>
            sessionStore.setModel(sessionId, m?.modelId ?? null)
          }
          permission={session?.permission ?? "ask"}
          onPermissionChange={(p) =>
            sessionStore.setPermission(sessionId, p)
          }
          webSearch={session?.web_search ?? false}
          onWebSearchChange={(v) =>
            sessionStore.setWebSearch(sessionId, v)
          }
          running={running}
          onStop={() => sessionStore.stop(sessionId)}
          onSubmit={() => {
            const txt = draft.trim();
            if (txt.length === 0 || running) {
              return;
            }

            setDraft("");
            handleSend(txt);
          }}
        />
      </div>
    </div>
  );
}
