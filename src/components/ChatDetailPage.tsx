import { useState } from "react";

import ChatInput from "./ChatInput";
import ChatTranscript from "./ChatTranscript";
import { useChatModels } from "../hooks/useChatModels";
import { sessionStore, useSessions } from "../stores/sessions";

type Props = { sessionId: string; onOpenAgent?: (id: string) => void };

export default function ChatDetailPage({ sessionId, onOpenAgent }: Props) {
  const { sessions, turns } = useSessions();
  const [draft, setDraft] = useState("");

  const turn = turns[sessionId];
  const running = turn !== undefined && turn.err === null;

  const { models } = useChatModels();
  const session = sessions.find((s) => s.id === sessionId);
  const model = models.find((m) => m.modelId === session?.model_id) ?? null;

  return (
    <div className="flex h-full min-h-0 w-full min-w-0 flex-col">
      <ChatTranscript sessionId={sessionId} onOpenAgent={onOpenAgent} />

      <div className="flex w-full shrink-0 justify-center pb-3 pt-2">
        <ChatInput
          placeholder="Reply to Argus…"
          value={draft}
          onChange={setDraft}
          model={model}
          onModelChange={(m) => sessionStore.setModel(sessionId, m?.modelId ?? null)}
          permission={session?.permission ?? "ask"}
          onPermissionChange={(p) => sessionStore.setPermission(sessionId, p)}
          webSearch={session?.web_search ?? false}
          onWebSearchChange={(v) => sessionStore.setWebSearch(sessionId, v)}
          running={running}
          onStop={() => sessionStore.stop(sessionId)}
          onSubmit={() => {
            const txt = draft.trim();
            if (txt.length === 0 || running) {
              return;
            }

            setDraft("");
            void sessionStore.send(sessionId, txt);
          }}
        />
      </div>
    </div>
  );
}
