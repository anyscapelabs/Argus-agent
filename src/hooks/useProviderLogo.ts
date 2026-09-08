import { useEffect, useState } from "react";
import { gwLogo } from "../lib/ipc";

// undefined = resolving, null = none, string = data URI ready
const cache = new Map<string, string | null>();

const toDataUri = (svg: string) => `data:image/svg+xml;utf8,${encodeURIComponent(svg)}`;

export function useProviderLogo(id: string) {
  const [uri, setUri] = useState<string | null | undefined>(cache.get(id));

  useEffect(() => {
    if (!id) {
      setUri(null);
      return;
    }
    if (cache.has(id)) {
      setUri(cache.get(id) ?? null);
      return;
    }
    let alive = true;
    gwLogo(id)
      .then((svg) => {
        const next = svg ? toDataUri(svg) : null;
        cache.set(id, next);
        if (alive) setUri(next);
      })
      .catch(() => {
        cache.set(id, null);
        if (alive) setUri(null);
      });
    return () => {
      alive = false;
    };
  }, [id]);

  return uri;
}
