import { convertFileSrc } from "@tauri-apps/api/core";
import { useSyncExternalStore } from "react";

import {
  fileStat,
  libraryPath,
  type Attachment,
  type LibItem,
} from "../lib/ipc";

export const MAX_FILES = 8;

// Images go as base64 in the request body, so they cost more than text.
export const MAX_IMAGE_BYTES = 4_000_000;
export const MAX_FILE_BYTES = 25_000_000;

export const IMAGE_EXT = ["png", "jpg", "jpeg", "gif", "webp"];

export type Pending = Attachment & { src: string | null };

type State = {
  items: Pending[];
  err: string | null;
  busy: boolean;
};

function isImage(ext: string): boolean {
  return IMAGE_EXT.includes(ext.toLowerCase());
}

function extOf(name: string): string {
  const at = name.lastIndexOf(".");
  return at < 0 ? "bin" : name.slice(at + 1).toLowerCase();
}

function baseName(path: string): string {
  const parts = path.split(/[\\/]/);
  return parts[parts.length - 1] || path;
}

class AttachStore {
  private state: State = { items: [], err: null, busy: false };

  private listeners = new Set<() => void>();

  subscribe = (fn: () => void) => {
    this.listeners.add(fn);
    return () => {
      this.listeners.delete(fn);
    };
  };

  getState = () => this.state;

  private set(patch: Partial<State>) {
    this.state = { ...this.state, ...patch };
    for (const fn of this.listeners) fn();
  }

  clear() {
    this.set({ items: [], err: null });
  }

  // The send never left the disk, so the chips come back as they were.
  restore(items: Attachment[]) {
    if (items.length === 0) return;

    this.set({
      items: [
        ...items.map((it) => ({
          ...it,
          src: it.path !== undefined && isImage(extOf(it.name))
            ? convertFileSrc(it.path)
            : null,
        })),
        ...this.state.items,
      ],
      err: null,
    });
  }

  remove(path: string) {
    this.set({ items: this.state.items.filter((it) => it.path !== path) });
  }

  // Never copied: it is already the user's file, and the message carries the
  // path that opens it.
  async addPaths(paths: string[]) {
    const room = MAX_FILES - this.state.items.length;
    if (room <= 0) {
      this.set({ err: `A message carries at most ${MAX_FILES} files.` });
      return;
    }

    this.set({ busy: true, err: null });
    const added: Pending[] = [];
    const refused: string[] = [];

    for (const path of paths.slice(0, room)) {
      const name = baseName(path);
      const ext = extOf(name);
      const cap = isImage(ext) ? MAX_IMAGE_BYTES : MAX_FILE_BYTES;

      try {
        const info = await fileStat(path);
        if (info.sz > cap) {
          refused.push(`${name} is over the ${cap} byte limit`);
          continue;
        }

        added.push({
          path: info.path,
          name,
          kind: info.kind,
          sz: info.sz,
          src: isImage(info.ext) ? convertFileSrc(info.path) : null,
        });
      } catch (e) {
        // The backend says why -- a refused name, a file that moved, a full disk.
        const why = e instanceof Error ? e.message : String(e);
        refused.push(`${name}: ${why}`);
      }
    }

    const overflow = paths.length > room ? ["and the rest"] : [];
    const why = [...refused, ...overflow];

    this.set({
      items: [...this.state.items, ...added],
      busy: false,
      err: why.length ? `Skipped ${why.join(", ")}.` : null,
    });
  }

  // Already in the library, so there is nothing to copy.
  async addLibItems(items: LibItem[]) {
    const room = MAX_FILES - this.state.items.length;
    if (room <= 0) {
      this.set({ err: `A message carries at most ${MAX_FILES} files.` });
      return;
    }

    this.set({ busy: true, err: null });
    const added: Pending[] = [];

    for (const item of items.slice(0, room)) {
      let path = "";
      try {
        path = await libraryPath(item.id);
      } catch {
        // The file is gone from disk. The row goes with it, so drop it.
        continue;
      }
      added.push({
        path,
        name: item.name,
        kind: item.kind,
        sz: item.sz,
        src: srcFor(item, path),
      });
    }

    this.set({
      items: [...this.state.items, ...added],
      busy: false,
      err: added.length ? null : "That file is no longer on disk.",
    });
  }

  // `src` is window-local and does not go on the wire.
  payload(): Attachment[] {
    return this.state.items.map(({ path, name, kind, sz }) => ({
      path,
      name,
      kind,
      sz,
    }));
  }
}

function srcFor(item: LibItem, path: string): string | null {
  return isImage(item.ext) ? convertFileSrc(path) : null;
}

export const attachStore = new AttachStore();

export function useAttachments(): State {
  return useSyncExternalStore(attachStore.subscribe, attachStore.getState);
}
