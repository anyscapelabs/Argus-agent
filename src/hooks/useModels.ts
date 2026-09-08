import { useCallback, useEffect, useMemo, useState } from "react";
import { gwProviderModels, gwSetModelEnabled, type ProviderModel } from "../lib/ipc";

export type ModelGroup = {
  providerId: string;
  models: ProviderModel[];
};

export function useModels() {
  const [models, setModels] = useState<ProviderModel[]>([]);
  const [loading, setLoading] = useState(true);
  const [err, setErr] = useState<string | null>(null);
  const [query, setQuery] = useState("");

  const refresh = useCallback(async () => {
    try {
      setModels(await gwProviderModels());
      setErr(null);
    } catch (e) {
      setErr(String(e));
    }
    setLoading(false);
  }, []);

  useEffect(() => {
    refresh();
  }, [refresh]);

  const toggle = useCallback(
    async (modelId: string, on: boolean) => {
      setModels((prev) => prev.map((m) => (m.modelId === modelId ? { ...m, enabled: on } : m)));
      try {
        await gwSetModelEnabled(modelId, on);
      } catch (e) {
        setErr(String(e));
        await refresh();
      }
    },
    [refresh],
  );

  const groups = useMemo(() => {
    const q = query.trim().toLowerCase();
    const byProvider = new Map<string, ProviderModel[]>();
    for (const m of models) {
      if (q && !m.displayName.toLowerCase().includes(q) && !m.modelId.toLowerCase().includes(q)) {
        continue;
      }
      const list = byProvider.get(m.providerId) ?? [];
      list.push(m);
      byProvider.set(m.providerId, list);
    }
    return [...byProvider.entries()]
      .map(([providerId, ms]) => ({ providerId, models: ms }))
      .sort((a, b) => a.providerId.localeCompare(b.providerId));
  }, [models, query]);

  return { groups, loading, err, query, setQuery, toggle, refresh };
}
