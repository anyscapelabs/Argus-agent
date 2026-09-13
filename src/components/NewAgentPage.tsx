import { useEffect, useState } from "react";

import { useChatModels } from "../hooks/useChatModels";
import type { ChatModel } from "../lib/ipc";
import { newagentPrefs, setNewagentPrefs } from "../lib/ipc";
import ChatInput from "./ChatInput";

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
      <ChatInput
        value={draft}
        onChange={setDraft}
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
          if (txt.length === 0) return;
          onSend(txt, model, permission, webSearch);
        }}
      />
    </div>
  );
}
