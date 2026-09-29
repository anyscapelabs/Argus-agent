import { useCallback, useEffect, useRef, useState } from "react";

import { gwChatModels, type ChatModel } from "../lib/ipc";

export function useChatModels() {
  const [models, setModels] = useState<ChatModel[]>([]);
  const [loading, setLoading] = useState(true);

  const aliveRef = useRef(true);

  const refresh = useCallback(async () => {
    try {
      const rows = await gwChatModels();
      if (!aliveRef.current) return;
      setModels(rows);
    } catch {
      if (!aliveRef.current) return;
      setModels([]);
    }

    if (aliveRef.current) setLoading(false);
  }, []);

  useEffect(() => {
    aliveRef.current = true;
    refresh();
    return () => {
      aliveRef.current = false;
    };
  }, [refresh]);

  return { models, loading, refresh };
}
