import { useCallback, useEffect, useState } from "react";
import { gwConnect, gwDisconnect, gwListProviders, gwSyncProviders, type Provider } from "../lib/ipc";

export function useProviders() {
  const [providers, setProviders] = useState<Provider[]>([]);
  const [loading, setLoading] = useState(true);
  const [syncing, setSyncing] = useState(false);
  const [err, setErr] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      setProviders(await gwListProviders());
      setErr(null);
    } catch (e) {
      setErr(String(e));
    }
    setLoading(false);
  }, []);

  useEffect(() => {
    refresh();
  }, [refresh]);

  const connect = useCallback(
    async (id: string, tok?: string) => {
      await gwConnect(id, tok);
      await refresh();
    },
    [refresh],
  );

  const disconnect = useCallback(
    async (id: string) => {
      await gwDisconnect(id);
      await refresh();
    },
    [refresh],
  );

  const syncCatalog = useCallback(async () => {
    setSyncing(true);
    try {
      await gwSyncProviders();
      await refresh();
    } catch (e) {
      setErr(String(e));
    }
    setSyncing(false);
  }, [refresh]);

  return { providers, loading, syncing, err, connect, disconnect, syncCatalog };
}
