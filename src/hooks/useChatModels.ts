import { useCallback, useEffect, useState } from "react";
import { gwChatModels, type ChatModel } from "../lib/ipc";

// Enabled models on connected providers — the set the chat selector offers.
export function useChatModels() {
  const [models, setModels] = useState<ChatModel[]>([]);
  const [loading, setLoading] = useState(true);

  const refresh = useCallback(async () => {
    try {
      setModels(await gwChatModels());
    } catch {
      setModels([]);
    }
    setLoading(false);
  }, []);

  useEffect(() => {
    refresh();
  }, [refresh]);

  return { models, loading, refresh };
}
