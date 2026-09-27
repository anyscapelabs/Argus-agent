import { useEffect, useState } from "react";

import { useChatModels } from "../hooks/useChatModels";
import type { ChatModel } from "../lib/ipc";
import {
  newagentPrefs,
  setNewagentPrefs,
  slashRun,
} from "../lib/ipc";
import { attachStore } from "../stores/attachments";
import type { Note } from "../stores/sessions";
import ChatInput from "./ChatInput";
import UsageCard from "./UsageCard";

type NewAgentPageProps = {
  onSend: (
    text: string,
    model: ChatModel | null,
    permission: string,
    webSearch: boolean,
  ) => void;
  initialPrompt?: string;
  onPromptUsed?: () => void;
};

export default function NewAgentPage({
  onSend,
  initialPrompt = "",
  onPromptUsed,
}: NewAgentPageProps) {
  const [draft, setDraft] = useState("");
  const [note, setNote] = useState<Note | null>(null);
  const { models } = useChatModels();
  const [modelId, setModelId] = useState<string | null>(null);
  const [permission, setPermission] = useState("ask");
  const [webSearch, setWebSearch] = useState(false);
  const model = modelId
    ? (models.find((m) => m.modelId === modelId) ?? null)
    : null;

  useEffect(() => {
    let alive = true;

    newagentPrefs()
      .then((p) => {
        if (!alive) return;

        setModelId(p.modelId);
        setPermission(p.permission);
        setWebSearch(p.webSearch);
      })
      .catch(() => {});

    return () => {
      alive = false;
    };
  }, []);

  useEffect(() => {
    if (initialPrompt !== "") {
      setDraft(initialPrompt);
      onPromptUsed?.();
    }
  }, []);

  const save = (next: {
    modelId: string | null;
    permission: string;
    webSearch: boolean;
  }) => {
    setNewagentPrefs(next).catch(() => {});
  };

  return (
    <div className="flex h-full w-full flex-col items-center justify-center gap-6 pb-40">
      <h1 className="font-serif text-3xl font-light text-text-primary">
        What should we work on?
      </h1>

      {note !== null && note.kind === "usage" && (
        <UsageCard initial={note.report} />
      )}

      {note !== null && note.kind === "text" && (
        <pre className="max-h-[40vh] w-[700px] max-w-full overflow-auto whitespace-pre-wrap rounded-xl border border-border-primary bg-bg-secondary px-3 py-2 font-mono text-xs text-text-primary">
          {note.text}
        </pre>
      )}
      <ChatInput
        value={draft}
        onChange={(next) => {
          setDraft(next);
          if (next.length === 0) setNote(null);
        }}
        model={model}
        onModelChange={(next) => {
          const id = next?.modelId ?? null;
          setModelId(id);
          save({ modelId: id, permission, webSearch });
        }}
        permission={permission}
        onPermissionChange={(next) => {
          setPermission(next);
          save({ modelId, permission: next, webSearch });
        }}
        webSearch={webSearch}
        onWebSearchChange={(next) => {
          setWebSearch(next);
          save({ modelId, permission, webSearch: next });
        }}
        onSubmit={() => {
          const txt = draft.trim();
          if (txt.length === 0 && attachStore.payload().length === 0) return;
          onSend(txt, model, permission, webSearch);
        }}
        onSlash={(name, arg) => {
          if (name === "clear") {
            attachStore.clear();
            setNote({ kind: "text", text: "Cleared the attached files." });
            return;
          }

          // No session yet, so a local command answers here and a macro has
          // to start one — the same path a typed prompt takes.
          void slashRun(name, null, arg)
            .then((out) => {
              if (out.usage !== null) {
                setNote({ kind: "usage", report: out.usage });
                return;
              }

              if (out.text !== null) {
                setNote({ kind: "text", text: out.text });
                return;
              }

              if (out.model !== null) {
                onSend(out.model, model, permission, webSearch);
              }
            })
            .catch((err) => setNote({ kind: "text", text: String(err) }));
        }}
      />
    </div>
  );
}
