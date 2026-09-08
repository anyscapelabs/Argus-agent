import { useEffect, useRef, useState } from "react";
import AgentBubble from "./AgentBubble";
import ChatInput from "./ChatInput";
import UserBubble from "./UserBubble";
import type { ChatMessage } from "../types/chat";

const SCROLL_LINE = 40;
const SCROLL_PAGE_RATIO = 0.85;

type Props = {
  messages: ChatMessage[];
  onSend: (text: string) => void;
  onRetry: (userMsgId: string) => void;
};

export default function ChatDetailPage({ messages, onSend, onRetry }: Props) {
  const scrollRef = useRef<HTMLDivElement | null>(null);
  const [draft, setDraft] = useState("");

  useEffect(() => {
    const el = scrollRef.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [messages]);

  const handleSend = () => {
    const txt = draft.trim();
    if (txt.length === 0) return;
    onSend(txt);
    setDraft("");
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
              <UserBubble key={message.id} timestamp={message.timestamp} onRetry={() => onRetry(message.id)}>
                {message.content}
              </UserBubble>
            ) : (
              <AgentBubble
                key={message.id}
                text={message.content}
                onRetry={() => {
                  const prev = messages[idx - 1];
                  if (prev && prev.role === "user") onRetry(prev.id);
                }}
              />
            )
          )}
        </div>
      </div>
      <div className="flex w-full shrink-0 justify-center pb-3 pt-2">
        <ChatInput
          placeholder="Reply to Argus…"
          value={draft}
          onChange={setDraft}
          onSubmit={handleSend}
        />
      </div>
    </div>
  );
}
