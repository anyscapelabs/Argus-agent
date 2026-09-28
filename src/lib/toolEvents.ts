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

// Steps built from event rows, not by re-parsing message text. Labels,
// details, and codes arrive computed; this maps them onto the card shapes
// the work panel already renders. Text parsing remains only for rows that
// predate events (legacy) and degraded turns (no usable API channel).
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

// Events for one message, or null when the message predates events and the
// caller must fall back to parsing its text.
export function eventsFor(
  events: Map<string, ToolEvent[]>,
  messageId: string,
): ToolEvent[] | null {
  const list = events.get(messageId);
  return list !== undefined && list.length > 0 ? list : null;
}

// A DocumentCard for a document event. This parses the backend's own
// `key=value` result lines — a stable internal format, not model prose —
// because the card predates events and still reads BlockNodes.
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
