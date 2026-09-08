import { useEffect, useRef, useState } from "react";
import AgentBubble from "./AgentBubble";
import ChatInput from "./ChatInput";
import UserBubble from "./UserBubble";
import { useChatModels } from "../hooks/useChatModels";
import { sessionStore, useSessions } from "../stores/sessions";
import type { ChatMessage } from "../types/chat";

const SCROLL_LINE = 40;
const SCROLL_PAGE_RATIO = 0.85;

type Props = { sessionId: string };

export default function ChatDetailPage({ sessionId }: Props) {
  const { sessions, msgs, turns } = useSessions();
  const scrollRef = useRef<HTMLDivElement | null>(null);
  const [draft, setDraft] = useState("");
  const rows = msgs[sessionId] ?? [];
  const turn = turns[sessionId];
  const running = turn !== undefined;

  const { models } = useChatModels();
  const session = sessions.find((s) => s.id === sessionId);
  const model = models.find((m) => m.modelId === session?.model_id) ?? null;

  const messages: ChatMessage[] = rows.map((m) => ({
    id: m.id,
    role: m.role === "user" ? "user" : "agent",
    content: m.content,
    timestamp: Date.now(),
  }));
  if (turn !== undefined) {
    messages.push({ id: "live", role: "agent", content: turn.text });
  }

  useEffect(() => {
    const el = scrollRef.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [messages.length, turn?.text]);

  const handleSend = (text: string) => {
    if (running) return;
    sessionStore.send(sessionId, text);
  };

  const handleRetry = (userMsgId: string) => {
    if (running) return;
    const idx = messages.findIndex((m) => m.id === userMsgId);
    if (idx === -1) return;
    const prev = messages[idx - 1];
    if (prev === undefined || prev.role !== "user") return;
    const row = rows.find((m) => m.id === userMsgId);
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
        className="min-h-0 flex-1 overflow-y-auto px-6 py-6 outline-none"
      >
        <div className="mx-auto flex w-full min-w-0 max-w-[700px] flex-col gap-3">
          {messages.map((message, idx) =>
            message.role === "user" ? (
              <UserBubble key={message.id} timestamp={message.timestamp} onRetry={() => handleRetry(message.id)}>
                {message.content}
              </UserBubble>
            ) : (
              <AgentBubble
                key={message.id}
                text={message.content}
                caret={running && message.id === "live"}
                onRetry={() => {
                  const prev = messages[idx - 1];
                  if (prev && prev.role === "user") handleRetry(prev.id);
                }}
              />
            )
          )}
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
