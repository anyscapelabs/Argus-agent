import { useEffect, useMemo, useRef, useState } from "react";
import { FiMinus, FiPlus } from "react-icons/fi";
import { LuBrain } from "react-icons/lu";

import type { Memory, MemoryGraph as Graph } from "../lib/ipc";

export function kindColor(kind: string): string {
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

type Pt = { x: number; y: number };

const MIN_ZOOM = 0.3;
const MAX_ZOOM = 2.5;
const RING = 14;

function degreesOf(data: Graph): Map<string, number> {
  const deg = new Map<string, number>();
  for (const n of data.nodes) deg.set(n.id, 0);
  for (const e of data.edges) {
    deg.set(e.from_id, (deg.get(e.from_id) ?? 0) + 1);
    deg.set(e.to_id, (deg.get(e.to_id) ?? 0) + 1);
  }
  return deg;
}

function layoutWorld(nodes: { id: string }[], deg: Map<string, number>): Map<string, Pt> {
  const out = new Map<string, Pt>();
  const sorted = [...nodes].sort(
    (a, b) => (deg.get(b.id) ?? 0) - (deg.get(a.id) ?? 0),
  );
  if (sorted.length === 0) return out;
  out.set(sorted[0].id, { x: 0, y: 0 });
  const rest = sorted.slice(1);
  rest.forEach((n, i) => {
    const ring = Math.floor(i / RING);
    const idx = i % RING;
    const count = Math.min(RING, rest.length - ring * RING);
    const r = 230 + ring * 175;
    const a = ((2 * Math.PI) / count) * idx - Math.PI / 2 + ring * 0.4;
    out.set(n.id, { x: r * Math.cos(a), y: r * 0.72 * Math.sin(a) });
  });
  return out;
}

function radiusOf(kind: string, deg: number): number {
  const base = kind === "session" ? 5 : 9;
  return base + Math.min(deg, 5);
}

export default function MemoryGraph({ data, mems }: { data: Graph; mems: Memory[] }) {
  const [view, setView] = useState({ x: 0, y: 0, k: 1 });
  const [offsets, setOffsets] = useState<Record<string, Pt>>({});
  const [sel, setSel] = useState<string | null>(null);
  const boxRef = useRef<HTMLDivElement | null>(null);
  const panRef = useRef<{ sx: number; sy: number; ox: number; oy: number; moved: boolean } | null>(null);
  const nodeRef = useRef<{ id: string; sx: number; sy: number } | null>(null);
  const fitted = useRef(false);

  const deg = useMemo(() => degreesOf(data), [data]);
  const base = useMemo(() => layoutWorld(data.nodes, deg), [data.nodes, deg]);

  const posOf = (id: string): Pt => {
    const b = base.get(id) ?? { x: 0, y: 0 };
    const o = offsets[id] ?? { x: 0, y: 0 };
    return { x: b.x + o.x, y: b.y + o.y };
  };

  useEffect(() => {
    const el = boxRef.current;
    if (!el || fitted.current || data.nodes.length === 0) return;
    fitted.current = true;
    const r = el.getBoundingClientRect();
    const k = Math.min(1, Math.min(r.width, r.height) / 720);
    setView({ x: r.width / 2, y: r.height / 2, k });
  }, [data.nodes.length]);

  useEffect(() => {
    const el = boxRef.current;
    if (!el) return;
    const onWheel = (e: WheelEvent) => {
      e.preventDefault();
      const rect = el.getBoundingClientRect();
      zoomAt(e.clientX - rect.left, e.clientY - rect.top, e.deltaY < 0 ? 1.12 : 1 / 1.12);
    };
    el.addEventListener("wheel", onWheel, { passive: false });
    return () => el.removeEventListener("wheel", onWheel);
  }, []);

  function zoomAt(cx: number, cy: number, f: number) {
    setView((v) => {
      const k = Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, v.k * f));
      const s = k / v.k;
      return { k, x: cx - (cx - v.x) * s, y: cy - (cy - v.y) * s };
    });
  }

  function zoomCenter(f: number) {
    const el = boxRef.current;
    if (!el) return;
    const r = el.getBoundingClientRect();
    zoomAt(r.width / 2, r.height / 2, f);
  }

  function reset() {
    const el = boxRef.current;
    if (!el) return;
    const r = el.getBoundingClientRect();
    const k = Math.min(1, Math.min(r.width, r.height) / 720);
    setOffsets({});
    setSel(null);
    setView({ x: r.width / 2, y: r.height / 2, k });
  }

  function onPanDown(e: React.PointerEvent<SVGSVGElement>) {
    e.currentTarget.setPointerCapture(e.pointerId);
    panRef.current = { sx: e.clientX, sy: e.clientY, ox: view.x, oy: view.y, moved: false };
  }

  function onPanMove(e: React.PointerEvent<SVGSVGElement>) {
    const p = panRef.current;
    if (!p) return;
    if (Math.abs(e.clientX - p.sx) + Math.abs(e.clientY - p.sy) > 3) p.moved = true;
    setView((v) => ({ ...v, x: p.ox + e.clientX - p.sx, y: p.oy + e.clientY - p.sy }));
  }

  function onPanUp() {
    if (panRef.current && !panRef.current.moved) setSel(null);
    panRef.current = null;
  }

  function onNodeDown(e: React.PointerEvent<SVGGElement>, id: string) {
    e.stopPropagation();
    e.currentTarget.setPointerCapture(e.pointerId);
    nodeRef.current = { id, sx: e.clientX, sy: e.clientY };
    setSel(id);
  }

  function onNodeMove(e: React.PointerEvent<SVGGElement>, id: string) {
    const n = nodeRef.current;
    if (!n || n.id !== id) return;
    const dx = (e.clientX - n.sx) / view.k;
    const dy = (e.clientY - n.sy) / view.k;
    n.sx = e.clientX;
    n.sy = e.clientY;
    setOffsets((prev) => ({
      ...prev,
      [id]: { x: (prev[id]?.x ?? 0) + dx, y: (prev[id]?.y ?? 0) + dy },
    }));
  }

  function onNodeUp() {
    nodeRef.current = null;
  }

  const selNode = sel !== null ? data.nodes.find((n) => n.id === sel) : undefined;
  const selMem =
    sel !== null && sel.startsWith("memory:")
      ? mems.find((m) => `memory:${m.id}` === sel)
      : undefined;
  const selEdges =
    sel !== null ? data.edges.filter((e) => e.from_id === sel || e.to_id === sel) : [];

  if (data.nodes.length === 0) {
    return (
      <div className="flex min-h-0 flex-1 flex-col items-center justify-center gap-2 px-6 py-10 text-center">
        <LuBrain size={26} className="text-text-secondary" />
        <p className="text-sm font-medium text-text-primary">No memories yet</p>
        <p className="max-w-xs text-xs text-text-secondary">
          Ask Argus to remember something and it will show up here.
        </p>
      </div>
    );
  }

  return (
    <div ref={boxRef} className="relative min-h-0 flex-1 overflow-hidden bg-bg-primary">
      <svg
        className="h-full w-full text-text-secondary"
        onPointerDown={onPanDown}
        onPointerMove={onPanMove}
        onPointerUp={onPanUp}
      >
        <defs>
          <pattern id="memdots" width="26" height="26" patternUnits="userSpaceOnUse">
            <circle cx="1.5" cy="1.5" r="1.5" fill="currentColor" opacity="0.14" />
          </pattern>
        </defs>
        <g transform={`translate(${view.x} ${view.y}) scale(${view.k})`}>
          <rect x={-2400} y={-2400} width={4800} height={4800} fill="url(#memdots)" />
          {data.edges.map((e, i) => {
            const a = posOf(e.from_id);
            const b = posOf(e.to_id);
            const hot = sel !== null && (e.from_id === sel || e.to_id === sel);
            return (
              <line
                key={`${e.from_id}-${e.to_id}-${i}`}
                x1={a.x}
                y1={a.y}
                x2={b.x}
                y2={b.y}
                stroke="currentColor"
                strokeOpacity={hot ? 0.7 : 0.25}
                strokeWidth={hot ? 1.5 : 1}
              />
            );
          })}
          {data.nodes.map((n) => {
            const p = posOf(n.id);
            const d = deg.get(n.id) ?? 0;
            const r = radiusOf(n.kind, d);
            const active = sel === n.id;
            return (
              <g
                key={n.id}
                onPointerDown={(e) => onNodeDown(e, n.id)}
                onPointerMove={(e) => onNodeMove(e, n.id)}
                onPointerUp={onNodeUp}
                className="cursor-grab"
              >
                <title>{n.label}</title>
                {active && (
                  <circle cx={p.x} cy={p.y} r={r + 6} fill="none" stroke={kindColor(n.kind)} strokeWidth={1.5} opacity={0.8} />
                )}
                <circle cx={p.x} cy={p.y} r={r} fill={kindColor(n.kind)} fillOpacity={0.9} />
                <text
                  x={p.x}
                  y={p.y + r + 15}
                  textAnchor="middle"
                  fontSize={11}
                  fill="currentColor"
                  opacity={active ? 1 : 0.75}
                >
                  {n.label.length > 26 ? `${n.label.slice(0, 26)}…` : n.label}
                </text>
              </g>
            );
          })}
        </g>
      </svg>
      {selNode && (
        <div className="absolute left-3 top-3 flex max-h-[calc(100%-24px)] w-80 max-w-[calc(100%-24px)] flex-col overflow-hidden rounded-lg border border-border-primary bg-bg-primary shadow-xl">
          <div className="shrink-0 px-3 pt-2">
            <div
              className="text-[11px] font-medium uppercase tracking-wide"
              style={{ color: kindColor(selNode.kind) }}
            >
              {selNode.kind}
            </div>
            {selMem ? (
              <p className="mt-0.5 max-h-40 overflow-y-auto whitespace-pre-wrap text-sm text-text-primary">
                {selMem.content}
              </p>
            ) : (
              <p className="mt-0.5 text-sm text-text-primary">{selNode.label}</p>
            )}
          </div>
          <div className="min-h-0 flex-1 overflow-y-auto px-3 py-2">
            <p className="text-[11px] font-medium uppercase tracking-wide text-text-secondary">
              Links · {selEdges.length}
            </p>
            {selEdges.map((e) => {
              const otherId = e.from_id === sel ? e.to_id : e.from_id;
              const other = data.nodes.find((n) => n.id === otherId);
              return (
                <div key={`${e.from_id}-${e.to_id}`} className="mt-1 text-xs text-text-secondary">
                  <span className="text-text-primary">{e.relation}</span>
                  {e.from_id === sel ? " → " : " ← "}
                  {other?.label ?? otherId}
                </div>
              );
            })}
          </div>
        </div>
      )}
      <div className="absolute bottom-3 right-3 flex items-center gap-1">
        <button
          type="button"
          onClick={() => zoomCenter(1.25)}
          aria-label="Zoom in"
          className="flex h-8 w-8 items-center justify-center rounded-lg border border-border-primary bg-bg-primary text-text-secondary hover:text-text-primary"
        >
          <FiPlus size={14} />
        </button>
        <button
          type="button"
          onClick={() => zoomCenter(1 / 1.25)}
          aria-label="Zoom out"
          className="flex h-8 w-8 items-center justify-center rounded-lg border border-border-primary bg-bg-primary text-text-secondary hover:text-text-primary"
        >
          <FiMinus size={14} />
        </button>
        <button
          type="button"
          onClick={reset}
          aria-label="Reset view"
          title="Reset view"
          className="h-8 rounded-lg border border-border-primary bg-bg-primary px-2 text-xs text-text-secondary hover:text-text-primary"
        >
          Reset
        </button>
      </div>
      <p className="pointer-events-none absolute bottom-3 left-3 text-[11px] text-text-secondary/70">
        Drag to pan · Scroll to zoom · Drag nodes to arrange
      </p>
    </div>
  );
}
