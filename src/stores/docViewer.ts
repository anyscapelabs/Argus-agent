import { useSyncExternalStore } from "react";

type State = {
  id: string | null;
};

class DocViewerStore {
  private state: State = { id: null };
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

  open(id: string) {
    this.set({ id });
  }

  close() {
    this.set({ id: null });
  }
}

export const docViewerStore = new DocViewerStore();

export function useDocViewer(): State {
  return useSyncExternalStore(
    docViewerStore.subscribe,
    docViewerStore.getState,
  );
}
