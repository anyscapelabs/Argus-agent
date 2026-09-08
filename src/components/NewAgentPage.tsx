import { useState } from "react";
import ChatInput from "./ChatInput";

type NewAgentPageProps = {
  onSend: (text: string) => void;
};

export default function NewAgentPage({ onSend }: NewAgentPageProps) {
  const [draft, setDraft] = useState("");
  return (
    <div className="flex h-full w-full flex-col items-center justify-center gap-6 pb-40">
      <h1 className="font-serif text-3xl font-light text-text-primary">
        What should we work on?
      </h1>
      <ChatInput value={draft} onChange={setDraft} onSubmit={() => onSend(draft)} />
    </div>
  );
}
