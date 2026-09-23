import { useEffect, useState } from "react";
import { LuBrain } from "react-icons/lu";

import type { Memory, MemoryGraph } from "../lib/ipc";
import { memoryDelete, memoryGraph, memoryList } from "../lib/ipc";
import { toast } from "../stores/toast";
import MemoryGraphView, { kindColor } from "./MemoryGraph";

export default function MemoryPage() {
  const [mems, setMems] = useState<Memory[]>([]);
  const [graph, setGraph] = useState<MemoryGraph>({ nodes: [], edges: [] });
  const [err, setErr] = useState<string | null>(null);
  const [mode, setMode] = useState<"list" | "graph">("graph");

  useEffect(() => {
    let alive = true;
    memoryList()
      .then((rows) => {
        if (alive) setMems(rows);
      })
      .catch((e) => {
        if (alive) setErr(String(e));
      });
    memoryGraph()
      .then((g) => {
        if (alive) setGraph(g);
      })
      .catch(() => {});
    return () => {
      alive = false;
    };
  }, []);

  const reload = () => {
    memoryList()
      .then(setMems)
      .catch(() => {});
    memoryGraph()
      .then(setGraph)
      .catch(() => {});
  };

  const remove = async (id: string) => {
    try {
      await memoryDelete(id);
      toast.success("Memory forgotten");
    } catch {
      toast.error("Delete failed");
      return;
    }
    reload();
  };

  return (
    <div className="relative flex h-full min-h-0 flex-col">
      <div className="absolute right-4 top-4 z-10 flex gap-1 rounded-lg border border-border-primary bg-bg-primary p-1">
        <button
          type="button"
          onClick={() => setMode("graph")}
          className={
            "rounded-md px-2 py-1 text-xs font-medium cursor-pointer " +
            (mode === "graph"
              ? "bg-bg-hover-primary text-text-primary"
              : "text-text-secondary")
          }
        >
          Graph
        </button>
        <button
          type="button"
          onClick={() => setMode("list")}
          className={
            "rounded-md px-2 py-1 text-xs font-medium cursor-pointer " +
            (mode === "list"
              ? "bg-bg-hover-primary text-text-primary"
              : "text-text-secondary")
          }
        >
          List
        </button>
      </div>
      {mode === "graph" && <MemoryGraphView data={graph} mems={mems} />}
      {mode === "list" && (
        <div className="mx-auto flex w-full max-w-4xl min-h-0 flex-1 flex-col gap-2 overflow-y-auto px-6 pb-8 pt-4">
          {err !== null && <p className="text-sm text-red-400">{err}</p>}
          {mems.map((m) => (
            <div
              key={m.id}
              className="rounded-xl border border-border-primary bg-bg-secondary px-3 py-2"
            >
              <div className="flex items-center justify-between gap-2">
                <span
                  className="text-[11px] font-medium uppercase tracking-wide"
                  style={{ color: kindColor(m.kind) }}
                >
                  {m.kind}
                </span>
                <button
                  type="button"
                  onClick={() => remove(m.id)}
                  className="text-xs text-text-secondary hover:text-text-primary cursor-pointer"
                  aria-label="Forget memory"
                >
                  Forget
                </button>
              </div>
              <p className="mt-1 text-sm text-text-primary">{m.content}</p>
            </div>
          ))}
          {mems.length === 0 && err === null && (
            <div className="flex flex-col items-center gap-2 px-6 py-10 text-center">
              <LuBrain size={26} className="text-text-secondary" />
              <p className="text-sm font-medium text-text-primary">
                No memories yet
              </p>
            </div>
          )}
        </div>
      )}
    </div>
  );
}
