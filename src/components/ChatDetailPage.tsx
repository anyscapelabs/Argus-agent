import { useEffect, useRef, useState } from "react";
import AgentBubble from "./AgentBubble";
import ChatInput from "./ChatInput";
import UserBubble from "./UserBubble";
import { useChatModels } from "../hooks/useChatModels";
import { sessionStore, useSessions, type Turn } from "../stores/sessions";
import type { MsgRow } from "../lib/ipc";

const SCROLL_LINE = 40;
const SCROLL_PAGE_RATIO = 0.85;

type Props = { sessionId: string };

type TurnGroup = { user: MsgRow | null; agent: MsgRow[] };

// One user message starts a turn; every agent-side row after it (assistant
// steps and tool results) belongs to that same turn and renders as one bubble.
function groupTurns(rows: MsgRow[], turn: Turn | undefined, sessionId: string): TurnGroup[] {
  const groups: TurnGroup[] = [];
  for (const r of rows) {
    if (r.role === "user" && !r.content.startsWith("<tool-result")) {
      groups.push({ user: r, agent: [] });
      continue;
    }
    if (groups.length === 0) groups.push({ user: null, agent: [] });
    groups[groups.length - 1].agent.push(r);
  }
  if (turn !== undefined && turn.err === null) {
    if (groups.length === 0) groups.push({ user: null, agent: [] });
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
  }
  return groups;
}

export default function ChatDetailPage({ sessionId }: Props) {
  const { sessions, msgs, turns } = useSessions();
  const scrollRef = useRef<HTMLDivElement | null>(null);
  const pinnedRef = useRef(true);
  const rafRef = useRef(0);
  const [draft, setDraft] = useState("");
  const rows = msgs[sessionId] ?? [];
  const turn = turns[sessionId];
  const running = turn !== undefined && turn.err === null;

  const { models } = useChatModels();
  const session = sessions.find((s) => s.id === sessionId);
  const model = models.find((m) => m.modelId === session?.model_id) ?? null;

  const groups = groupTurns(rows, turn, sessionId);

  const voteOf = (msgId: string) => rows.find((m) => m.id === msgId)?.vote ?? null;

  // Follow the stream only while the user sits near the bottom; scrolling up
  // unpins until they come back down.
  useEffect(() => {
    const onScroll = () => {
      const el = scrollRef.current;
      if (el) pinnedRef.current = el.scrollHeight - el.scrollTop - el.clientHeight < 80;
    };
    const el = scrollRef.current;
    if (el) {
      el.addEventListener("scroll", onScroll, { passive: true });
      return () => el.removeEventListener("scroll", onScroll);
    }
  }, []);

  useEffect(() => {
    pinnedRef.current = true;
    const el = scrollRef.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [sessionId]);

  useEffect(() => {
    if (!pinnedRef.current) return;
    cancelAnimationFrame(rafRef.current);
    rafRef.current = requestAnimationFrame(() => {
      const el = scrollRef.current;
      if (el !== null && pinnedRef.current) el.scrollTop = el.scrollHeight;
    });
    return () => cancelAnimationFrame(rafRef.current);
  }, [rows.length, turn?.text]);

  const handleSend = (text: string) => {
    if (running) return;
    sessionStore.send(sessionId, text);
  };

  const retryFrom = (userMsgId: string) => {
    if (running) return;
    const row = rows.find((m) => m.id === userMsgId && m.role === "user");
    if (row === undefined) return;
    sessionStore.retry(sessionId, row.seq, row.content);
  };

  const handleScrollKey = (event: React.KeyboardEvent<HTMLDivElement>) => {
    const el = scrollRef.current;
    if (!el) return;
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
        <div className="mx-auto flex w-full min-w-0 max-w-[700px] flex-col gap-3">
          {groups.map((group, gi) => {
            const text = group.agent
              .filter((a) => a.role === "assistant")
              .map((a) => a.content)
              .join("\n\n");
            const last = [...group.agent].reverse().find((a) => a.role === "assistant");
            const live = running && gi === groups.length - 1;
            return (
              <div key={group.user?.id ?? `g-${gi}`} className="flex flex-col gap-3">
                {group.user !== null && (
                  <UserBubble
                    timestamp={Date.now()}
                    onRetry={() => {
                      if (group.user !== null) retryFrom(group.user.id);
                    }}
                  >
                    {group.user.content}
                  </UserBubble>
                )}
                <AgentBubble
                  text={text}
                  caret={live}
                  vote={live ? null : voteOf(last?.id ?? "")}
                  onVote={(v) => {
                    if (last !== undefined) sessionStore.setVote(sessionId, last.id, v);
                  }}
                  onRetry={() => {
                    if (group.user !== null) retryFrom(group.user.id);
                  }}
                />
              </div>
            );
          })}
          {turn?.err !== undefined && turn !== undefined && turn.err !== null && (
            <div className="rounded-xl border border-red-500/30 bg-red-500/10 px-3 py-2 text-sm text-red-400">
              {turn.err}
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
          onModelChange={(m) => sessionStore.setModel(sessionId, m.modelId)}
          permission={session?.permission ?? "ask"}
          onPermissionChange={(p) => sessionStore.setPermission(sessionId, p)}
          webSearch={session?.web_search ?? false}
          onWebSearchChange={(v) => sessionStore.setWebSearch(sessionId, v)}
          onSubmit={() => {
            const txt = draft.trim();
            if (txt.length === 0 || running) return;
            setDraft("");
            handleSend(txt);
          }}
        />
      </div>
    </div>
  );
}
