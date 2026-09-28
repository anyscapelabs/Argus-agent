import { describe, expect, it } from "bun:test";

import { parse } from "./agentXml";

// What the user reads is whatever this returns. Every case below is either a
// tag that leaked into prose or prose that was read as a tag; both are the
// same bug seen from opposite ends.

type Simple = { kind: string; tag: string; text: string };

function simple(xml: string): Simple[] {
  return parse(xml).map((b) => ({
    kind: b.kind,
    tag: b.tag,
    text: b.children.map((c) => c.value).join(""),
  }));
}

describe("prose that only looks like markup", () => {
  it("leaves a comparison alone", () => {
    expect(simple("if a < b and c > d then")).toEqual([
      { kind: "paragraph", tag: "p", text: "if a < b and c > d then" },
    ]);
  });

  it("leaves a bare less-than alone", () => {
    expect(simple("5 < 6")).toEqual([
      { kind: "paragraph", tag: "p", text: "5 < 6" },
    ]);
  });

  it("does not let a tag eat the line after it", () => {
    const out = simple("<step\nthe answer is 42</step>");

    expect(out.map((b) => b.text).join(" ")).toContain("the answer is 42");
  });
});

describe("a tag arriving on a stream", () => {
  it("reads a half-typed tag as text until it closes", () => {
    expect(simple("the answer\n<ste")).toEqual([
      { kind: "paragraph", tag: "p", text: "the answer" },
      { kind: "paragraph", tag: "p", text: "<ste" },
    ]);
  });

  it("reads a step once its opening tag has landed", () => {
    expect(simple("<step>read the fi")).toEqual([
      { kind: "component", tag: "step", text: "read the fi" },
    ]);
  });

  it("keeps the rest of the reply while a step is still open", () => {
    const out = simple("<step>read it</step>\ndone");

    expect(out).toEqual([
      { kind: "component", tag: "step", text: "read it" },
      { kind: "paragraph", tag: "p", text: "done" },
    ]);
  });
});

describe("markdown inside a prose tag", () => {
  it("renders a step as markdown, not as raw syntax", () => {
    const out = parse("<step>the **bold** part</step>");
    const step = out[0].children.map((c) => c.value).join("");

    expect(step).toContain("<bold>bold</bold>");
  });

  it("keeps a payload body raw so a shell comment survives", () => {
    const out = parse(
      '<terminal cmd="ls"># not a heading\n-rw-r--r--  1 me  me 12 f</terminal>',
    );

    expect(out[0].tag).toBe("terminal");
    expect(out[0].children.map((c) => c.value).join("")).toContain(
      "# not a heading",
    );
  });
});

describe("a sub-agent's card", () => {  it("survives a turn that carried one", () => {
    const out = simple(
      '<agent id="a1" name="scout" state="running">go</agent>\nfound it',
    );

    expect(out[0].tag).toBe("agent");
  });

  it("carries the report through to the next turn", () => {
    const out = simple('<agent-done id="a1" name="scout">all clear</agent-done>');

    expect(out[0].tag).toBe("agent-done");
    expect(out[0].text).toContain("all clear");
  });
});

describe("numeric entities round-trip terminal commands", () => {
  it("decodes a newline entity back to a newline", () => {
    const out = parse(
      '<terminal id="a1" command="for f&#10;done" status="success">out</terminal>',
    );

    expect(out[0].attrs.command).toBe("for f\ndone");
  });

  it("decodes a redirect entity back to >", () => {
    const out = parse(
      '<terminal id="a1" command="grep foo &gt; out" status="success">out</terminal>',
    );

    expect(out[0].attrs.command).toBe("grep foo > out");
  });

  it("an unterminated tag does not suppress later markdown", () => {
    const out = parse('before\n<terminal cmd="ls\n# Heading\n**bold**');

    expect(out.some((b) => b.tag === "h2" || b.tag === "h3")).toBe(true);
    expect(out.map((b) => b.children.map((c) => c.value).join("")).join(" "))
      .toContain("<bold>bold</bold>");
  });

  it("a warning keeps inline markup for the banner to render", () => {
    const out = parse("<warning>Use <code>diff</code> here</warning>");

    expect(out[0].tag).toBe("warning");
    expect(out[0].children.map((c) => c.value).join("")).toContain(
      "<code>diff</code>",
    );
  });

  it("a zero-width space never blocks a heading", () => {
    const out = parse("\u200b# Heading");

    expect(out.some((b) => b.tag === "h2" || b.tag === "h3")).toBe(true);
  });
});
