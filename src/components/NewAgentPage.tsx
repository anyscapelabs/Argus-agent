import { useEffect, useState } from "react";

import { useChatModels } from "../hooks/useChatModels";
import type { ChatModel } from "../lib/ipc";
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

const MODEL_KEY = "argus.newagent.model";
const PERM_KEY = "argus.newagent.permission";
const WEB_KEY = "argus.newagent.websearch";

export default function NewAgentPage({
  onSend,
  initialPrompt = "",
  onPromptUsed,
}: NewAgentPageProps) {
  const [draft, setDraft] = useState("");
  const { models } = useChatModels();
  const [modelId, setModelId] = useState<string | null>(() =>
    localStorage.getItem(MODEL_KEY),
  );
  const [permission, setPermission] = useState(
    () => localStorage.getItem(PERM_KEY) ?? "ask",
  );
  const [webSearch, setWebSearch] = useState(
    () => localStorage.getItem(WEB_KEY) === "true",
  );
  const model = modelId
    ? (models.find((m) => m.modelId === modelId) ?? null)
    : null;

  useEffect(() => {
    if (initialPrompt !== "") {
      setDraft(initialPrompt);
      onPromptUsed?.();
    }
  }, []);

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

          if (id === null) {
            localStorage.removeItem(MODEL_KEY);
          } else {
            localStorage.setItem(MODEL_KEY, id);
          }
        }}
        permission={permission}
        onPermissionChange={(next) => {
          setPermission(next);
          localStorage.setItem(PERM_KEY, next);
        }}
        webSearch={webSearch}
        onWebSearchChange={(next) => {
          setWebSearch(next);
          localStorage.setItem(WEB_KEY, String(next));
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
