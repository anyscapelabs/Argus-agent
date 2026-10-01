import { convertFileSrc } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";

import { libraryPath, type Attachment } from "../lib/ipc";

// A library file has only an id, so the transcript asks for the path once and
// remembers it: reopening a chat must not re-ask for every image in it.
const CACHE = new Map<string, string>();

export function libSrc(id: string): string | null {
  const hit = CACHE.get(id);
  return hit !== undefined ? convertFileSrc(hit) : null;
}

export function useLibSrc(id: string | null): string | null {
  const [src, setSrc] = useState<string | null>(() =>
    id === null ? null : libSrc(id),
  );

  useEffect(() => {
    if (id === null || CACHE.has(id)) {
      return;
    }

    let live = true;
    void libraryPath(id)
      .then((path) => {
        CACHE.set(id, path);
        if (live) setSrc(convertFileSrc(path));
      })
      .catch(() => {
        // File gone; the chip falls back to an icon, not a broken image.
      });

    return () => {
      live = false;
    };
  }, [id]);

  return id === null ? null : (src ?? libSrc(id));
}

export function useFileSrc(file: Attachment): string | null {
  const viaId = useLibSrc(file.kind === "image" ? (file.id ?? null) : null);

  return viaId ?? (file.path !== undefined ? convertFileSrc(file.path) : null);
}
