import { useState } from "react";

import ChatInput from "./ChatInput";
import ChatTranscript from "./ChatTranscript";
import { useChatModels } from "../hooks/useChatModels";
import { slashRun } from "../lib/ipc";
import { attachStore } from "../stores/attachments";
import { isWorking, sessionStore, useSessions } from "../stores/sessions";

type Props = { sessionId: string; onOpenAgent?: (id: string) => void };

export default function ChatDetailPage({ sessionId, onOpenAgent }: Props) {
  const st = useSessions();
  const { sessions } = st;
  const [draft, setDraft] = useState("");

  // A parent waiting on sub-agents is busy even though its own turn is over,
  // and the stop button has to be there for it.
  const running = isWorking(st, sessionId);

  const { models } = useChatModels();
  const session = sessions.find((s) => s.id === sessionId);
  const model = models.find((m) => m.modelId === session?.model_id) ?? null;

  return (
    <div className="flex h-full min-h-0 w-full min-w-0 flex-col">
      <ChatTranscript sessionId={sessionId} onOpenAgent={onOpenAgent} />

      <div className="flex w-full shrink-0 justify-center pb-3 pt-2">
        <ChatInput
          sessionId={sessionId}
          placeholder="Reply to Argus…"
          value={draft}
          onChange={setDraft}
          model={model}
          onModelChange={(m) =>
            sessionStore.setModel(sessionId, m?.modelId ?? null)
          }
          permission={session?.permission ?? "ask"}
          onPermissionChange={(p) => sessionStore.setPermission(sessionId, p)}
          webSearch={session?.web_search ?? false}
          onWebSearchChange={(v) => sessionStore.setWebSearch(sessionId, v)}
          running={running}
          onStop={() => sessionStore.stop(sessionId)}
          onSubmit={() => {
            const txt = draft.trim();
            // An empty body with chips on it is not an empty turn.
            const bare = txt.length === 0 && attachStore.payload().length === 0;
            if (bare || running) {
              return;
            }

            setDraft("");
            void sessionStore.send(sessionId, txt);
          }}
          onSlash={(name, arg) => {
            // Goes in as a real turn: it survives a reload and the model is
            // never told about it.
            if (name === "usage") {
              void sessionStore.runLocal(sessionId, name, arg).catch((err) => {
                sessionStore.addNote(sessionId, {
                  kind: "text",
                  text: String(err),
                });
              });
              return;
            }

            void slashRun(name, sessionId, arg)
              .then((out) => {
                if (out.text !== null) {
                  sessionStore.addNote(sessionId, {
                    kind: "text",
                    text: out.text,
                  });
                  return;
                }

                // An ordinary turn with different words.
                if (out.model !== null) {
                  void sessionStore.send(sessionId, out.model);
                }
              })
              .catch((err) => {
                sessionStore.addNote(sessionId, {
                  kind: "text",
                  text: String(err),
                });
              });
          }}
        />
      </div>
    </div>
  );
}
