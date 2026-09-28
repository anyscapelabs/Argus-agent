import { describe, expect, it } from "bun:test";

import type { ToolEvent } from "./ipc";
import {
  eventsFor,
  eventStep,
  fallbackCounts,
  hasRecordBlocks,
  noteFallback,
  type CountStore,
} from "./toolEvents";

function ev(part: Partial<ToolEvent>): ToolEvent {
  return {
    id: "e1",
    message_id: "m1",
    session_id: "s1",
    kind: "action",
    tool: "grep",
    args_json: "{}",
    status: "succeeded",
    elapsed_ms: 0,
    code: 0,
    output: "",
    label: "",
    detail: "",
    created_at: "",
    ...part,
  };
}

describe("steps from event rows", () => {
  it("maps a terminal event without re-parsing markup", () => {
    const s = eventStep(
      ev({
        kind: "terminal",
        tool: "terminal",
        label: "$ ls",
        output: "exit 0\na",
        code: 0,
        elapsed_ms: 120,
      }),
    );

    expect(s.group).toBe("terminal");
    expect(s.label).toBe("$ ls");
    expect(s.code).toBe(0);
    expect(s.durationMs).toBe(120);
    expect(s.output).toContain("exit 0");
  });

  it("marks denied terminals like the text path did", () => {
    const s = eventStep(ev({ kind: "terminal", code: -2, label: "$ x" }));

    expect(s.code).toBe(-2);
  });

  it("maps sandbox, browser, and document rows", () => {
    expect(
      eventStep(ev({ kind: "sandbox", label: "$ cmd", detail: "sandboxed" }))
        .badge,
    ).toBe("sandboxed");
    expect(eventStep(ev({ kind: "browser", label: "Opened https://x" })).label)
      .toBe("Opened https://x");
    expect(
      eventStep(ev({ kind: "document", label: "Created document N" })).label,
    ).toBe("Created document N");
  });

  it("keeps email args for the draft card", () => {
    const s = eventStep(
      ev({
        tool: "gmail.send",
        args_json: '{"to":"a@b.c","subject":"Hi","body":"Yo"}',
        label: "Email to a@b.c",
      }),
    );

    expect(s.tool).toBe("gmail.send");
    expect(s.args?.["to"]).toBe("a@b.c");
  });

  it("falls back to labels, never empty rows", () => {
    expect(eventStep(ev({ tool: "mystery" })).label).toBe("Ran mystery");
  });
});

describe("legacy fallback selection", () => {
  it("returns rows when present, null when the message predates events", () => {
    const map = new Map([["m1", [ev({})]]]);

    expect(eventsFor(map, "m1")?.length).toBe(1);
    expect(eventsFor(map, "m-old")).toBeNull();
    expect(eventsFor(new Map(), "m1")).toBeNull();
  });
});

describe("fallback metrics", () => {
  function memStore(): CountStore {
    let v: string | null = null;
    return {
      get: () => v,
      set: (s: string) => {
        v = s;
      },
    };
  }

  it("splits suspicious from legacy and counts each message once", () => {
    expect(hasRecordBlocks(["p", "terminal"])).toBe(true);
    expect(hasRecordBlocks(["p", "thinking"])).toBe(false);
    expect(hasRecordBlocks([])).toBe(false);

    const store = memStore();
    const id = `m-${Date.now()}-${Math.random()}`;
    noteFallback(id, true, ["terminal"], store);
    noteFallback(id, true, ["terminal"], store);
    noteFallback(`${id}-old`, false, ["action"], store);

    expect(fallbackCounts(store)).toEqual({ suspicious: 1, legacy: 1 });
  });

  it("ignores prose-only fallbacks", () => {
    const store = memStore();
    noteFallback(`m-prose-${Date.now()}`, true, ["p", "h2"], store);

    expect(fallbackCounts(store)).toEqual({ suspicious: 0, legacy: 0 });
  });

  it("stays silent with no storage at all", () => {
    noteFallback(`m-null-${Date.now()}`, true, ["terminal"], null);

    expect(fallbackCounts(null)).toEqual({ suspicious: 0, legacy: 0 });
  });
});
