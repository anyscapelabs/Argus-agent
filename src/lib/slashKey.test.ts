import { describe, expect, it } from "bun:test";

import { menuTakesEnter, SLASH_CALL, SLASH_MENU } from "./slashKey";

const CMDS = [
  { name: "usage" },
  { name: "clear" },
  { name: "model" },
];

describe("menuTakesEnter", () => {
  it("leaves Enter alone on an ordinary message", () => {
    // The regression. An unscoped match against the registry hits the first
    // command here, because every name starts with the empty string, and Enter
    // stops sending on every message the moment one command exists.
    for (const text of [
      "hello",
      "fix the bug in src/lib/ipc.ts",
      "what does /usage do",
      "a/b",
      "  leading space",
    ]) {
      expect(menuTakesEnter(text, CMDS)).toBe(false);
    }
  });

  it("gives Enter to the menu when it has something to complete", () => {
    expect(menuTakesEnter("/", CMDS)).toBe(true);
    expect(menuTakesEnter("/u", CMDS)).toBe(true);
    expect(menuTakesEnter("/usage", CMDS)).toBe(true);
  });

  it("sends when the menu is open but has no match", () => {
    // A keystroke that completes nothing would otherwise go nowhere.
    expect(menuTakesEnter("/zzz", CMDS)).toBe(false);
  });

  it("sends when the registry is empty", () => {
    expect(menuTakesEnter("/u", [])).toBe(false);
  });
});

describe("SLASH_MENU", () => {
  it("only opens at the start of the line with no newline after it", () => {
    expect(SLASH_MENU.test("/usage")).toBe(true);
    expect(SLASH_MENU.test("/")).toBe(true);

    expect(SLASH_MENU.test("src/lib")).toBe(false);
    expect(SLASH_MENU.test("text /usage")).toBe(false);
    expect(SLASH_MENU.test("/usage arg")).toBe(false);
    expect(SLASH_MENU.test("/usage\nmore")).toBe(false);
  });
});

describe("SLASH_CALL", () => {
  it("reads a whole-line command with an argument", () => {
    const m = SLASH_CALL.exec("/usage today");
    expect(m?.[1]).toBe("usage");
    expect(m?.[2]).toBe("today");
  });

  it("takes a bare command as no argument", () => {
    // The group is optional, so a bare command yields undefined rather than
    // an empty string. That is why the caller writes `?? ""`.
    const m = SLASH_CALL.exec("/clear");
    expect(m?.[1]).toBe("clear");
    expect(m?.[2]).toBeUndefined();
    expect(m?.[2] ?? "").toBe("");
  });

  it("leaves a path or a sentence alone", () => {
    expect(SLASH_CALL.exec("src/lib")).toBeNull();
    expect(SLASH_CALL.exec("what does /usage do")).toBeNull();
  });
});
