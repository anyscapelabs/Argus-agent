import { useState } from "react";
import type { ChatModel } from "../lib/ipc";
import ChatInput from "./ChatInput";

type NewAgentPageProps = {
  onSend: (text: string, model: ChatModel | null) => void;
};

export default function NewAgentPage({ onSend }: NewAgentPageProps) {
  const [draft, setDraft] = useState("");
  const [model, setModel] = useState<ChatModel | null>(null);

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
        onSubmit={() => {
          const txt = draft.trim();
          if (txt.length === 0) return;
          onSend(txt, model);
        }}
      />
    </div>
  );
}
