import { useEffect, useMemo, useState } from "react";
import { LuBrain, LuSearch } from "react-icons/lu";

import type { Memory, MemoryGraph } from "../lib/ipc";
import { memoryDelete, memoryGraph, memoryList, memorySearch } from "../lib/ipc";
import { toast } from "../stores/toast";

function layout(nodes: { id: string }[]) {
  const n = nodes.length;
  const cx = 260;
  const cy = 170;
  const rx = 210;
  const ry = 125;
  return nodes.map((node, i) => {
    if (n === 1) return { id: node.id, x: cx, y: cy };
    const a = (2 * Math.PI * i) / n - Math.PI / 2;
    return { id: node.id, x: cx + rx * Math.cos(a), y: cy + ry * Math.sin(a) };
  });
}

function kindColor(kind: string) {
  switch (kind) {
    case "preference":
      return "#a78bfa";
    case "project":
      return "#60a5fa";
    case "person":
      return "#f472b6";
    case "decision":
      return "#fbbf24";
    case "session":
      return "#64748b";
    default:
      return "#34d399";
  }
}

export default function MemoryPage() {
  const [mems, setMems] = useState<Memory[]>([]);
  const [graph, setGraph] = useState<MemoryGraph>({ nodes: [], edges: [] });
  const [query, setQuery] = useState("");
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

  useEffect(() => {
    if (query.trim() === "") {
      memoryList()
        .then(setMems)
        .catch(() => {});
      return;
    }
    const t = setTimeout(() => {
      memorySearch(query.trim())
        .then(setMems)
        .catch(() => {});
    }, 200);
    return () => clearTimeout(t);
  }, [query]);

  const pos = useMemo(() => layout(graph.nodes), [graph.nodes]);
  const posById = useMemo(() => new Map(pos.map((p) => [p.id, p])), [pos]);

  const remove = async (id: string) => {
    try {
      await memoryDelete(id);
      toast.success("Memory forgotten");
    } catch {
      toast.error("Delete failed");
      return;
    }
    setMems((prev) => prev.filter((m) => m.id !== id));
    setGraph((prev) => ({
      nodes: prev.nodes.filter((n) => n.id !== `memory:${id}`),
      edges: prev.edges.filter(
        (e) => e.from_id !== `memory:${id}` && e.to_id !== `memory:${id}`,
      ),
    }));
  };

  return (
    <div className="mx-auto w-full max-w-2xl py-4">
      <div className="mb-1 flex items-center justify-between">
        <h1 className="text-2xl font-medium text-text-primary">Memory</h1>
        <div className="flex gap-1 rounded-lg border border-border-primary p-1">
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
      </div>
      <p className="mb-5 text-sm font-medium text-text-secondary">
        Argus recalls memories, past messages, summaries and files.
      </p>
      <div
        className={
          "flex h-10 w-full items-center rounded-full border " +
          "border-border-primary bg-bg-secondary px-4"
        }
      >
        <LuSearch size={18} className="shrink-0 text-text-secondary" />
        <input
          type="text"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder="Search memory..."
          className={
            "ml-2 flex-1 bg-transparent text-sm text-text-primary " +
            "outline-none placeholder:text-text-secondary"
          }
        />
      </div>
      {err !== null && <p className="mt-4 text-sm text-red-400">{err}</p>}
      {mode === "graph" && (
        <div className="mt-6 rounded-xl border border-border-primary bg-bg-secondary p-2">
          {graph.nodes.length === 0 ? (
            <div className="flex flex-col items-center gap-2 px-6 py-10 text-center">
              <LuBrain size={26} className="text-text-secondary" />
              <p className="text-sm font-medium text-text-primary">
                No memories yet
              </p>
              <p className="max-w-xs text-xs text-text-secondary">
                Ask Argus to remember something and it will show up here.
              </p>
            </div>
          ) : (
            <svg viewBox="0 0 520 340" className="h-80 w-full">
              {graph.edges.map((e, i) => {
                const a = posById.get(e.from_id);
                const b = posById.get(e.to_id);
                if (!a || !b) return null;
                return (
                  <line
                    key={`${e.from_id}-${e.to_id}-${i}`}
                    x1={a.x}
                    y1={a.y}
                    x2={b.x}
                    y2={b.y}
                    stroke="currentColor"
                    strokeOpacity={0.25}
                    strokeWidth={1}
                  />
                );
              })}
              {graph.nodes.map((n) => {
                const p = posById.get(n.id);
                if (!p) return null;
                return (
                  <g key={n.id}>
                    <circle
                      cx={p.x}
                      cy={p.y}
                      r={n.kind === "session" ? 6 : 10}
                      fill={kindColor(n.kind)}
                      fillOpacity={0.85}
                    />
                    <text
                      x={p.x}
                      y={p.y + 22}
                      textAnchor="middle"
                      fontSize={9}
                      fill="currentColor"
                      opacity={0.7}
                    >
                      {n.label.length > 22
                        ? `${n.label.slice(0, 22)}…`
                        : n.label}
                    </text>
                  </g>
                );
              })}
            </svg>
          )}
        </div>
      )}
      {mode === "list" && (
        <div className="mt-6 flex flex-col gap-2">
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
                {query.trim() === "" ? "No memories yet" : "No matches"}
              </p>
            </div>
          )}
        </div>
      )}
    </div>
  );
}
