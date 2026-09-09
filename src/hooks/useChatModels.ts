import { useCallback, useEffect, useState } from "react";

import { gwChatModels, type ChatModel } from "../lib/ipc";

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
