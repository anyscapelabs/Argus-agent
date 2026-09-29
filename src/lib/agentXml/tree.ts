// Assembling tokens into a tree, and deciding which blocks are the work log
// rather than the answer. `WORK_TAGS` is the single source of truth for that
// split, so the panel and the renderer cannot disagree about it.
import { Char, classifyAt, isSpaceAt } from "./scan";
import type { Token } from "./tokenize";

export type InlineNode = {
  kind: "text";
  value: string;
};

export type BlockNode = {
  kind: "component" | "paragraph" | "heading";
  tag: string;
  attrs: Record<string, string>;
  children: InlineNode[];
};

export type XmlTree = BlockNode[];

// Single source of truth for block ownership. The work panel renders `work`
// blocks; bubbles render everything else, plus `work` only when no panel
// owns the turn (fallback so tools can never vanish).
const WORK_TAGS = new Set([
  "action",
  "terminal",
  "sandbox",
  "browser-action",
  "check",
  "document",
  "plan",
  "thinking",
  "step",
]);

export function blockRole(tag: string): "work" | "content" {
  return WORK_TAGS.has(tag) ? "work" : "content";
}

export function buildTree(toks: Token[]): XmlTree {
  const blks: BlockNode[] = [];
  let buf = "";
  let cur: BlockNode | null = null;
  const inline = new Set([
    "bold",
    "italic",
    "underline",
    "strikethrough",
    "code",
    "link",
  ]);
  const tableInner = new Set(["tr", "th", "td"]);

  const flush = () => {
    const v = buf.trim();
    buf = "";
    if (v.length === 0) return;

    // A bullet or a number followed by a space. Read rather than matched: it
    // runs for every line of every block, and it is the only place the tree
    // builder decides that prose is a list.
    const isList = (l: string): boolean => {
      const c = l.charCodeAt(0);

      if (c === 0x2d || c === 0x2a || c === 0x2022) {
        return isSpaceAt(l, 1);
      }

      let k = 0;
      while (k < l.length) {
        const d = classifyAt(l, k);
        if (d & Char.Digit) {
          k++;
          continue;
        }
        break;
      }
      if (k === 0 || l.charCodeAt(k) !== 0x2e) return false;

      return isSpaceAt(l, k + 1);
    };
    let group: string[] = [];

    const pushGroup = () => {
      if (group.length === 0) return;
      blks.push({
        kind: "paragraph",
        tag: "p",
        attrs: {},
        children: [{ kind: "text", value: group.join("\n") }],
      });
      group = [];
    };

    for (const raw of v.split("\n")) {
      const line = raw.trim();
      if (line.length === 0) continue;

      if (isList(line)) {
        group.push(line);
        continue;
      }

      pushGroup();
      blks.push({
        kind: "paragraph",
        tag: "p",
        attrs: {},
        children: [{ kind: "text", value: line }],
      });
    }

    pushGroup();
  };

  for (const tok of toks) {
    if (tok.kind === "text") {
      if (cur) {
        cur.children.push({ kind: "text", value: tok.value });
        continue;
      }
      buf += tok.value;
      continue;
    }

    if (tok.kind === "open") {
      const tag = tok.tag;

      if (inline.has(tag)) {
        let raw = `<${tag}>`;

        if (tag === "link") {
          const href = tok.attrs.href ?? "";
          raw = href ? `<link href="${href}">` : "<link>";
        }

        if (cur) {
          cur.children.push({ kind: "text", value: raw });
          continue;
        }

        buf += raw;
        continue;
      }

      if (
        cur &&
        (cur.tag === "table" || cur.tag === "tr") &&
        tableInner.has(tag)
      ) {
        const raw = `<${tag}>`;
        cur.children.push({ kind: "text", value: raw });
        continue;
      }

      flush();

      if (cur) cur = null;

      const kind: BlockNode["kind"] =
        tag === "h1" || tag === "h2" || tag === "h3" || tag === "h4"
          ? "heading"
          : "component";

      cur = { kind, tag, attrs: tok.attrs, children: [] };
      blks.push(cur);
      continue;
    }

    const tag = tok.tag;

    if (inline.has(tag)) {
      const raw = `</${tag}>`;
      if (cur) {
        cur.children.push({ kind: "text", value: raw });
        continue;
      }
      buf += raw;
      continue;
    }

    if (
      cur &&
      (cur.tag === "table" || cur.tag === "tr") &&
      tableInner.has(tag)
    ) {
      const raw = `</${tag}>`;
      cur.children.push({ kind: "text", value: raw });
      continue;
    }

    if (!cur) continue;

    if (tag === "") {
      buf = "";
      cur = null;
      continue;
    }

    if (cur.tag !== tag) {
      // A closing tag that does not match — `</arg_value>` for an `<action>` —
      // must not leave the block open. While `cur` lives, every following line
      // is appended to it, and a block that renders as one line swallows the
      // rest of the reply. End it here and let the prose out.
      cur = null;
      continue;
    }

    buf = "";
    cur = null;
  }

  flush();

  return blks.filter((blk) => {
    if (blk.kind !== "paragraph") return true;
    return blk.children.some((c) => c.value.trim().length > 0);
  });
}
