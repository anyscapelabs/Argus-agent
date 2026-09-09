import { useEffect, useState } from "react";

import { gwLogo } from "../lib/ipc";

const LOGO_CACHE = new Map<string, string | null>();

const toDataUri = (svg: string) =>
  `data:image/svg+xml;utf8,${encodeURIComponent(svg)}`;

export function useProviderLogo(id: string) {
  const [uri, setUri] = useState<string | null | undefined>(
    LOGO_CACHE.get(id),
  );

  useEffect(() => {
    if (!id) {
      setUri(null);
      return;
    }

    if (LOGO_CACHE.has(id)) {
      setUri(LOGO_CACHE.get(id) ?? null);
      return;
    }

    let alive = true;

    gwLogo(id)
      .then((svg) => {
        const next = svg ? toDataUri(svg) : null;
        LOGO_CACHE.set(id, next);
        if (alive) setUri(next);
      })
      .catch(() => {
        LOGO_CACHE.set(id, null);
        if (alive) setUri(null);
      });

    return () => {
      alive = false;
    };
  }, [id]);

  return uri;
}
