import { useEffect, useState } from "react";

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

export default function NewAgentPage({
  onSend,
  initialPrompt = "",
  onPromptUsed,
}: NewAgentPageProps) {
  const [draft, setDraft] = useState("");
  const [model, setModel] = useState<ChatModel | null>(null);
  const [permission, setPermission] = useState("ask");
  const [webSearch, setWebSearch] = useState(false);

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
        onModelChange={setModel}
        permission={permission}
        onPermissionChange={setPermission}
        webSearch={webSearch}
        onWebSearchChange={setWebSearch}
        onSubmit={() => {
          const txt = draft.trim();
          if (txt.length === 0) return;
          onSend(txt, model, permission, webSearch);
        }}
      />
    </div>
  );
}
