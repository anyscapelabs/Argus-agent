import { describe, expect, it } from "bun:test";

import { parse } from "./agentXml";
import { normalizeMd } from "./agentXml/markdown";
import { tokenize } from "./agentXml/lexer";

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

describe("a sub-agent's card", () => {
  it("survives a turn that carried one", () => {
    const out = simple(
      '<agent id="a1" name="scout" state="running">go</agent>\nfound it',
    );

    expect(out[0].tag).toBe("agent");
  });

  it("carries the report through to the next turn", () => {
    const out = simple(
      '<agent-done id="a1" name="scout">all clear</agent-done>',
    );

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
    expect(
      out.map((b) => b.children.map((c) => c.value).join("")).join(" "),
    ).toContain("<bold>bold</bold>");
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
    expect(toks.some((t) => t.kind === "open" && t.tag === "terminal")).toBe(
      true,
    );
    expect(toks.some((t) => t.kind === "close" && t.tag === "terminal")).toBe(
      true,
    );
  });

  it("releases the held tag when the closing bracket arrives", () => {
    const held = tokenize('<terminal id="a1" command="ls', { final: false });
    expect(held.some((t) => t.kind === "text")).toBe(false);

    const complete = tokenize('<terminal id="a1" command="ls">out</terminal>', {
      final: false,
    });
    expect(
      complete.some((t) => t.kind === "open" && t.tag === "terminal"),
    ).toBe(true);
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

  it("does not hide the message behind a < that never closes", () => {
    // The worst case, and the one that made a chat look like it had stopped
    // parsing: a `<` with a letter after it and no `>` for the rest of the
    // reply. Only the last line of a streaming message can still grow, so a
    // lone `<` on an earlier line is prose and must not hold anything back.
    const msg =
      "Compare with <h and read on.\n\n# Later heading\n\nThe closing sentence.";
    const t = parse(msg, { final: false });
    const text = t.map((b) => b.children.map((c) => c.value).join("")).join(" ");

    expect(text).toContain("The closing sentence.");
    expect(t.some((b) => b.tag === "h2")).toBe(true);
  });

  it("keeps every line of a live message except a mid-tag last line", () => {
    // A tag still arriving on the final line is held; nothing before it is.
    const msg = "# Title\n\nprose\n\n<terminal id=\"a1\" command=\"ls";
    const t = parse(msg, { final: false });
    const text = t.map((b) => b.children.map((c) => c.value).join("")).join(" ");

    expect(text).toContain("Title");
    expect(text).toContain("prose");
    expect(text).not.toContain("<terminal");
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
    expect(normalizeMd("a ~~gone~~ b")).toBe(
      "a <strikethrough>gone</strikethrough> b",
    );
  });

  it("leaves an unclosed marker alone", () => {
    expect(normalizeMd("a **not closed")).toBe("a **not closed");
  });

  it("does not italicise snake_case", () => {
    expect(normalizeMd("call snake_case_name now")).toBe(
      "call snake_case_name now",
    );
  });

  it("renders a code span literally", () => {
    expect(normalizeMd("use `a **b** c` here")).toBe(
      "use <code>a **b** c</code> here",
    );
  });

  it("only links an http destination", () => {
    expect(normalizeMd("[t](https://x.com)")).toBe(
      '<link href="https://x.com">t</link>',
    );
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
    expect(normalizeMd("| A | B |\n|---|---|", { final: false })).toContain(
      "<table>",
    );
  });

  it("keeps an escaped pipe inside a cell", () => {
    const out = normalizeMd("| A | B |\n|---|---|\n| a \\| b | 2 |");
    expect(out).toContain("<td>a | b</td>");
  });
});

// The two shapes that were reported as broken, kept as whole messages so a
// regression in one part of the pipeline shows up as a bad render rather than
// as a passing unit test.

const REPORT = `All folder inspection, READMEs, git metadata, and sizes are gathered — the audit is complete. Here is the report.

# Code Projects Audit (read-only — nothing was moved or modified)

## 1. What each folder is, and its state

| Folder | What it is | Git? | Size |
|---|---|---|---|
| ~/ai-tutor | AI tutoring platform. Bun monorepo. | Yes — remote | 409M |

The clip has from <1000 lines, from zero. The route is a -> b, and x && y.

### 2. What to do next

Nothing was modified. Run \`cd ~/ai-tutor\` to start.`;

const rendered = (t: ReturnType<typeof parse>): string =>
  t.map((b) => b.children.map((c) => c.value).join("")).join(" ");

describe("a report like a real audit", () => {
  it("renders headings and the table, and keeps a bare < as prose", () => {
    const t = parse(REPORT);
    const kinds = t.map((b) => b.tag);

    expect(kinds).toContain("table");
    expect(kinds.filter((k) => k === "h2").length).toBe(2);
    expect(kinds).toContain("h3");

    const text = rendered(t);
    // The heading markers are markup now, not text.
    expect(text).not.toContain("# Code Projects");
    expect(text).not.toContain("## 1. What each");
    // A comparison inside prose is still a comparison.
    expect(text).toContain("<1000 lines");
  });

  it("shows no raw markdown at any point while it streams in", () => {
    for (let n = 1; n <= REPORT.length; n += 7) {
      const text = rendered(parse(REPORT.slice(0, n), { final: false }));

      if (
        text.includes("<table") ||
        text.includes("<h2") ||
        text.includes("<h3")
      ) {
        continue;
      }

      // A line that has not been recognised as a table or heading yet must
      // still not read as a half-rendered one.
      expect(/^#{1,3} /m.test(text)).toBe(false);
      expect(text).not.toContain("|---");
    }
  });
});

describe("prose components the bubble used to drop", () => {
  it("holds back an unclosed fence mid-stream, renders it on final", () => {
    const live = normalizeMd("```js\nconst a = 1;", { final: false });
    expect(live).not.toContain("```");
    expect(live).not.toContain("<codeblock");

    const done = normalizeMd("```js\nconst a = 1;", { final: true });
    expect(done).toContain("<codeblock");
  });

  it("keeps quote markers for the renderer instead of flattening", () => {
    expect(normalizeMd("> quoted")).toBe("> quoted");
    expect(normalizeMd("plain")).toBe("plain");
  });

  it("keeps a dash rule as a break, drops the rest as before", () => {
    expect(normalizeMd("---")).toBe("---");
    expect(normalizeMd("***")).toBe("");
  });

  it("passes images through as markup instead of deleting them", () => {
    const out = normalizeMd("see ![alt](https://x.test/i.png) now");
    expect(out).toContain("<img");
    expect(out).toContain('src="https://x.test/i.png"');
    expect(normalizeMd("see ![alt](/home/u/lion.png) now")).toContain(
      '<img src="/home/u/lion.png"',
    );
    expect(normalizeMd("see ![alt](relative/i.png) now")).not.toContain(
      "<img",
    );
    expect(normalizeMd("see ![alt](//evil.test/i.png) now")).not.toContain(
      "<img",
    );
  });

  it("keeps an image inline in the paragraph that holds it", () => {
    const tags = parse("see ![a](https://x.test/i.png) now").map((b) => b.tag);
    expect(tags).not.toContain("img");
  });
});
