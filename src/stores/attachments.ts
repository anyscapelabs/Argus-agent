import { convertFileSrc } from "@tauri-apps/api/core";
import { useSyncExternalStore } from "react";

import {
  libraryAdd,
  libraryPath,
  type Attachment,
  type LibItem,
} from "../lib/ipc";

// A message carries a handful of files, not a folder. Beyond this the chip
// row stops being a row.
export const MAX_FILES = 8;

// An image is sent to the model as base64 in the request body, so it costs
// more than a document, which costs a tool call instead.
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

// The library appends the extension itself, so a name carrying one would land
// on disk as `photo.png.png`.
function stemOf(name: string): string {
  const at = name.lastIndexOf(".");
  return at <= 0 ? name : name.slice(0, at);
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

  // A send that never got off the ground. The ids are still the library's, so
  // the chips come back exactly as they were.
  restore(items: Attachment[]) {
    if (items.length === 0) return;

    this.set({
      items: [
        ...items.map((it) => ({ ...it, src: null })),
        ...this.state.items,
      ],
      err: null,
    });
  }

  remove(id: string) {
    this.set({ items: this.state.items.filter((it) => it.id !== id) });
  }

  // Off the user's disk, the dialog or a drop. Argus copies each one into the
  // library, because the original is allowed to move.
  async addPaths(paths: string[], sessionId?: string) {
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
        const item = await libraryAdd(path, stemOf(name), sessionId);
        if (item.sz > cap) {
          refused.push(`${name} is over the ${cap} byte limit`);
          continue;
        }
        // The chip and the note both want the name the user recognises, which
        // is the one with the extension on it.
        added.push({
          ...toAttachment(item),
          name,
          src: srcFor(item, path),
        });
      } catch (e) {
        // The backend says why -- a name it refused, a file that moved, a
        // disk that is full. "Could not be added" is none of those.
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
      added.push({ ...toAttachment(item), src: srcFor(item, path) });
    }

    this.set({
      items: [...this.state.items, ...added],
      busy: false,
      err: added.length ? null : "That file is no longer on disk.",
    });
  }

  // What goes on the wire. The display path is dropped: the backend records an
  // id, because a path can move.
  payload(): Attachment[] {
    return this.state.items.map(({ id, name, kind, sz }) => ({
      id,
      name,
      kind,
      sz,
    }));
  }
}

function toAttachment(item: LibItem): Attachment {
  return { id: item.id, name: item.name, kind: item.kind, sz: item.sz };
}

function srcFor(item: LibItem, path: string): string | null {
  return isImage(item.ext) ? convertFileSrc(path) : null;
}

export const attachStore = new AttachStore();

export function useAttachments(): State {
  return useSyncExternalStore(attachStore.subscribe, attachStore.getState);
}
