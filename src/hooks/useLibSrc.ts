import { convertFileSrc } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";

import { libraryPath } from "../lib/ipc";

// A message records a library id, never a path, because a path can move. So
// the transcript asks for the path once per id and remembers it: reopening a
// chat must not re-ask the backend for every image in it.
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
        // The file is gone. The row goes with it, and the chip falls back to
        // an icon rather than a broken image.
      });

    return () => {
      live = false;
    };
  }, [id]);

  return id === null ? null : (src ?? libSrc(id));
}
