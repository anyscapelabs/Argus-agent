import { describe, expect, it } from "bun:test";

import { parse } from "./agentXml";
import { normalizeMd } from "./agentXml/markdown";
import { tokenize } from "./agentXml/tokenize";

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

// Streaming: text arrives a delta at a time, so the parser sees tags that are
// not finished. Showing a fragment as raw markup is what put a whole
// `<terminal …` command in the chat as literal text.

describe("streaming holds back an unterminated tag", () => {
  it("does not leak a half-arrived terminal tag as text", () => {
    let text = "";
    const full =
      '<terminal id="a1" command="cd ~ && ls -lh bitmap.png" status="ok" duration_ms="0">exit 0</terminal>';

    // Every prefix of the message must never render the tag as prose.
    for (let n = 1; n <= full.length; n++) {
      text = full.slice(0, n);
      const toks = tokenize(text, { final: false });
      const prose = toks
        .filter((t) => t.kind === "text")
        .map((t) => (t as { value: string }).value)
        .join("");
      expect(prose).not.toContain("<terminal");
    }
  });

  it("still emits the tag once it has closed", () => {
    const full =
      '<terminal id="a1" command="cd ~" status="ok" duration_ms="0">exit 0</terminal>';
    const toks = tokenize(full, { final: false });
    expect(toks.some((t) => t.kind === "open" && t.tag === "terminal")).toBe(true);
    expect(toks.some((t) => t.kind === "close" && t.tag === "terminal")).toBe(true);
  });

  it("releases the held tag when the closing bracket arrives", () => {
    const held = tokenize('<terminal id="a1" command="ls', { final: false });
    expect(held.some((t) => t.kind === "text")).toBe(false);

    const complete = tokenize('<terminal id="a1" command="ls">out</terminal>', {
      final: false,
    });
    expect(complete.some((t) => t.kind === "open" && t.tag === "terminal")).toBe(true);
  });

  it("treats an unterminated tag as prose once the message is final", () => {
    const toks = tokenize("a < b and c > d", { final: true });
    const prose = toks
      .filter((t) => t.kind === "text")
      .map((t) => (t as { value: string }).value)
      .join("");
    expect(prose).toContain("a < b and c > d");
  });

  it("does not hold back prose that only looks like a tag", () => {
    // `<1000` is a comparison, not the start of a tag, so it must render.
    const toks = tokenize("a file is <1000 lines", { final: false });
    const prose = toks
      .filter((t) => t.kind === "text")
      .map((t) => (t as { value: string }).value)
      .join("");
    expect(prose).toContain("<1000 lines");
  });

  it("does not hang on a stray < with no partner", () => {
    const toks = tokenize("2 < 3 and never closes", { final: false });
    const prose = toks
      .filter((t) => t.kind === "text")
      .map((t) => (t as { value: string }).value)
      .join("");
    expect(prose).toContain("2 < 3");
  });
});

describe("ATX headings", () => {
  it("allows up to three spaces of indentation", () => {
    expect(normalizeMd("# Title")).toBe("<h2>Title</h2>");
    expect(normalizeMd(" # Title")).toBe("<h2>Title</h2>");
    expect(normalizeMd("   # Title")).toBe("<h2>Title</h2>");
  });

  it("does not treat four spaces as a heading", () => {
    expect(normalizeMd("    # Title")).toBe("    # Title");
  });

  it("requires a space after the hashes", () => {
    expect(normalizeMd("#NoSpace")).toBe("#NoSpace");
  });

  it("strips a closing run of hashes", () => {
    expect(normalizeMd("## Title ##")).toBe("<h2>Title</h2>");
  });

  it("keeps a hash that is part of the content", () => {
    expect(normalizeMd("## C# in depth")).toBe("<h2>C# in depth</h2>");
  });

  it("maps depth onto the two heading levels", () => {
    expect(normalizeMd("### Three")).toBe("<h3>Three</h3>");
    expect(normalizeMd("###### Six")).toBe("<h3>Six</h3>");
  });
});

describe("inline formatting", () => {
  it("bolds, italics and strikes", () => {
    expect(normalizeMd("a **bold** b")).toBe("a <bold>bold</bold> b");
    expect(normalizeMd("a ~~gone~~ b")).toBe("a <strikethrough>gone</strikethrough> b");
  });

  it("leaves an unclosed marker alone", () => {
    expect(normalizeMd("a **not closed")).toBe("a **not closed");
  });

  it("does not italicise snake_case", () => {
    expect(normalizeMd("call snake_case_name now")).toBe("call snake_case_name now");
  });

  it("renders a code span literally", () => {
    expect(normalizeMd("use `a **b** c` here")).toBe("use <code>a **b** c</code> here");
  });

  it("only links an http destination", () => {
    expect(normalizeMd("[t](https://x.com)")).toBe('<link href="https://x.com">t</link>');
    expect(normalizeMd("[t](javascript:alert(1))")).toBe("<link>t</link>");
  });
});

describe("tables", () => {
  it("rebuilds a GFM table", () => {
    const out = normalizeMd("| A | B |\n|---|---|\n| 1 | 2 |");
    expect(out).toBe(
      "<table><tr><th>A</th><th>B</th></tr><tr><td>1</td><td>2</td></tr></table>",
    );
  });

  it("holds the header until the delimiter row arrives", () => {
    // Mid-stream the header row is still a paragraph; the next chunk makes it
    // a table. Rendering it early would flash a table that is not one yet.
    expect(normalizeMd("| A | B |", { final: false })).toBe("| A | B |");
    expect(normalizeMd("| A | B |\n|---|---|", { final: false })).toContain("<table>");
  });

  it("keeps an escaped pipe inside a cell", () => {
    const out = normalizeMd("| A | B |\n|---|---|\n| a \\| b | 2 |");
    expect(out).toContain("<td>a | b</td>");
  });
});
