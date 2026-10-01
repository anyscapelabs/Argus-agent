import type { BlockNode } from "./agentXml";
import type { ToolEvent } from "./ipc";
import type { ToolStep } from "../components/agent/ToolActivity";

function parseArgs(raw: string): Record<string, unknown> {
  try {
    const v: unknown = JSON.parse(raw);
    if (v !== null && typeof v === "object" && !Array.isArray(v)) {
      return v as Record<string, unknown>;
    }
    return {};
  } catch {
    return {};
  }
}

// `suspicious` = event rows exist yet a message fell back to markup.
// `legacy` = pre-events session. Read when deciding whether the text splice goes.
const COUNT_KEY = "argus.metrics.fallback";
const counted = new Set<string>();

export type FallbackCounts = { suspicious: number; legacy: number };

export type CountStore = {
  get(): string | null;
  set(v: string): void;
};

function domStore(): CountStore | null {
  if (typeof localStorage === "undefined") return null;

  try {
    return {
      get: () => localStorage.getItem(COUNT_KEY),
      set: (v: string) => localStorage.setItem(COUNT_KEY, v),
    };
  } catch {
    return null;
  }
}

export function fallbackCounts(store: CountStore | null = domStore()): FallbackCounts {
  const zero = { suspicious: 0, legacy: 0 };
  if (store === null) return zero;

  try {
    const raw = store.get();
    if (raw === null) return zero;
    const cur = JSON.parse(raw) as Partial<FallbackCounts>;
    return {
      suspicious: cur.suspicious ?? 0,
      legacy: cur.legacy ?? 0,
    };
  } catch {
    return zero;
  }
}

const RECORD_TAGS = new Set([
  "action",
  "terminal",
  "sandbox",
  "browser-action",
  "document",
  "check",
]);

export function hasRecordBlocks(tags: string[]): boolean {
  return tags.some((t) => RECORD_TAGS.has(t));
}

export function noteFallback(
  messageId: string,
  sessionHasEvents: boolean,
  tags: string[],
  store: CountStore | null = domStore(),
): void {
  if (!hasRecordBlocks(tags) || counted.has(messageId)) return;
  counted.add(messageId);
  if (counted.size > 5000) {
    const first = counted.values().next();
    if (!first.done) counted.delete(first.value);
  }
  if (store === null) return;

  try {
    const cur = fallbackCounts(store);
    const key = sessionHasEvents ? "suspicious" : "legacy";
    store.set(JSON.stringify({ ...cur, [key]: cur[key] + 1 }));
  } catch {
    // Never break rendering.
  }
}

// Steps from event rows, not by re-parsing text. Labels, details and codes
// arrive computed.
export function eventStep(ev: ToolEvent): ToolStep {
  const durationMs = ev.elapsed_ms > 0 ? ev.elapsed_ms : undefined;
  const output = ev.output.length > 0 ? ev.output : undefined;

  if (ev.kind === "terminal") {
    return {
      group: "terminal",
      label: ev.label || "$ shell",
      detail: ev.detail || undefined,
      output,
      code: ev.code,
      durationMs,
    };
  }

  if (ev.kind === "sandbox") {
    return {
      group: "sandbox",
      label: ev.label || "$ command",
      detail: ev.detail || undefined,
      output,
      badge: "sandboxed",
    };
  }

  if (ev.kind === "browser") {
    return { group: "browser", label: ev.label || "Browsed" };
  }

  if (ev.kind === "document") {
    return { group: "tool", label: ev.label || "Created document" };
  }

  if (ev.tool === "gmail.send" || ev.tool === "outlook.send") {
    return {
      group: "tool",
      tool: ev.tool,
      args: parseArgs(ev.args_json),
      label: ev.label || "Email",
    };
  }

  return { group: "tool", tool: ev.tool, label: ev.label || `Ran ${ev.tool}` };
}

// Null when the message predates events: the caller parses its text instead.
export function eventsFor(
  events: Map<string, ToolEvent[]>,
  messageId: string,
): ToolEvent[] | null {
  const list = events.get(messageId);
  return list !== undefined && list.length > 0 ? list : null;
}

// Parses the backend's own `key=value` result lines — stable internal format,
// not model prose — because the card predates events and reads BlockNodes.
export function docBlockFor(ev: ToolEvent): BlockNode | null {
  if (ev.kind !== "document") return null;

  const field = (key: string): string => {
    const line = ev.output.split("\n").find((l) => l.startsWith(key));
    return line === undefined ? "" : line.slice(key.length).trim();
  };
  const title = field("title=") || field("name=") || "Untitled document";

  return {
    kind: "component",
    tag: "document",
    attrs: {
      id: field("id="),
      title,
      doctype: field("ext=") || "docx",
      pages: field("pages="),
      status: "ready",
    },
    children: [],
  };
}
