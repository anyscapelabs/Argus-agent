import { useEffect, useState } from "react";

import { useChatModels } from "../hooks/useChatModels";
import type { ChatModel } from "../lib/ipc";
import {
  newagentPrefs,
  setNewagentPrefs,
  slashRun,
} from "../lib/ipc";
import { attachStore } from "../stores/attachments";
import ChatInput from "./ChatInput";

type NewAgentPageProps = {
  onSend: (
    text: string,
    model: ChatModel | null,
    permission: string,
    webSearch: boolean,
  ) => void;
  /// A command the app answers itself. It still needs a chat to live in, so the
  /// page starts one and puts the turn in it.
  onRunLocal: (
    name: string,
    arg: string,
    model: ChatModel | null,
    permission: string,
    webSearch: boolean,
  ) => void;
  initialPrompt?: string;
  onPromptUsed?: () => void;
};

export default function NewAgentPage({
  onSend,
  onRunLocal,
  initialPrompt = "",
  onPromptUsed,
}: NewAgentPageProps) {
  const [draft, setDraft] = useState("");
  const [note, setNote] = useState<string | null>(null);
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

      {note !== null && (
        <pre className="max-h-[40vh] w-[700px] max-w-full overflow-auto whitespace-pre-wrap rounded-xl border border-border-primary bg-bg-secondary px-3 py-2 font-mono text-xs text-text-primary">
          {note}
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
            setNote("Cleared the attached files.");
            return;
          }

          // The turn needs a chat to live in.
          if (name === "usage") {
            onRunLocal(name, arg, model, permission, webSearch);
            return;
          }

          // No session yet: the same path a typed prompt takes.
          void slashRun(name, null, arg)
            .then((out) => {
              if (out.text !== null) {
                setNote(out.text);
                return;
              }

              if (out.model !== null) {
                onSend(out.model, model, permission, webSearch);
              }
            })
            .catch((err) => setNote(String(err)));
        }}
      />
    </div>
  );
}
