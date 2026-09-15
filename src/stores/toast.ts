import { useSyncExternalStore } from "react";

export type ToastKind = "success" | "error" | "info";

export type ToastItem = {
  id: number;
  kind: ToastKind;
  msg: string;
};

const TTL = 3500;
let nextId = 1;

class ToastStore {
  private items: ToastItem[] = [];
  private listeners = new Set<() => void>();
  private timers = new Map<number, ReturnType<typeof setTimeout>>();

  subscribe = (fn: () => void) => {
    this.listeners.add(fn);
    return () => {
      this.listeners.delete(fn);
    };
  };

  getState = () => this.items;

  private set(items: ToastItem[]) {
    this.items = items;
    for (const fn of this.listeners) fn();
  }

  private push(kind: ToastKind, msg: string) {
    const id = nextId++;
    this.set([...this.items.slice(-3), { id, kind, msg }]);
    const t = setTimeout(() => this.dismiss(id), TTL);
    this.timers.set(id, t);
  }

  success(msg: string) {
    this.push("success", msg);
  }

  error(msg: string) {
    this.push("error", msg);
  }

  info(msg: string) {
    this.push("info", msg);
  }

  dismiss(id: number) {
    const t = this.timers.get(id);
    if (t !== undefined) {
      clearTimeout(t);
      this.timers.delete(id);
    }
    this.set(this.items.filter((item) => item.id !== id));
  }
}

export const toast = new ToastStore();

export function useToasts(): ToastItem[] {
  return useSyncExternalStore(toast.subscribe, toast.getState);
}
