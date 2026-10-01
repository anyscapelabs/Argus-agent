// Parser: tokens into a tree (buildTree, blockRole) + parse entry with bounded cache. One place from text to XmlTree.
import { Char, classifyAt, isSpaceAt } from "./lexer";
import type { Token } from "./lexer";
import { normalizeMd } from "./markdown";
import { tokenize } from "./lexer";
// Tokens into a tree, and which blocks are the work log rather than the answer.

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

// The work panel renders these; bubbles render everything else, plus these only
// when no panel owns the turn, so tools can never vanish.
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

    // A bullet or a number followed by a space. Char-read, not regex: this runs
    // for every line of every block.
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
      // An unmatched close — `</arg_value>` for an `<action>` — must not leave the
      // block open: a one-line block would swallow the rest of the reply.
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

// Cached: the chat re-renders the same finished reply on every keystroke.

const PARSE_CAP = 1_000_000;

/**
 * A model emits one of these mid-tag and the tag never closes. A scan, not a
 * regex: this runs on every delta.
 */
const INVISIBLE = new Set([
  0x200b, 0x200c, 0x200d, 0xfeff, 0x00ad, 0x200e, 0x200f, 0x202a, 0x202b,
  0x202c, 0x202d, 0x202e, 0x2060, 0x2061, 0x2062, 0x2063, 0x2064, 0x2065,
  0x2066, 0x2067, 0x2068, 0x2069, 0x206a, 0x206b, 0x206c, 0x206d, 0x206e,
  0x206f,
]);

function stripInvisible(s: string): string {
  const first = findInvisible(s, 0);
  if (first === -1) return s;

  let out = s.slice(0, first);
  for (let i = first; i < s.length; i++) {
    const code = s.charCodeAt(i);
    if (INVISIBLE.has(code)) continue;
    out += s[i];
  }
  return out;
}

function findInvisible(s: string, from: number): number {
  for (let i = from; i < s.length; i++) {
    if (INVISIBLE.has(s.charCodeAt(i))) return i;
  }
  return -1;
}

/** `false` on a live turn holds an unfinished tag back instead of flashing raw markup. */
export function parse(buf: string, opts: { final?: boolean } = {}): XmlTree {
  const final = opts.final !== false;
  // Rust strips these too, but streamed text lands here first.
  const clean = stripInvisible(buf);
  const src = clean.length > PARSE_CAP ? clean.slice(0, PARSE_CAP) : clean;

  try {
    return buildTree(tokenize(normalizeMd(src, { final }), { final }));
  } catch {
    return [
      {
        kind: "paragraph",
        tag: "p",
        attrs: {},
        children: [{ kind: "text", value: src }],
      },
    ];
  }
}

const PARSE_CACHE = new Map<string, XmlTree>();

/** The cache is keyed by finality too: a live buffer and a settled one parse differently. */
const cacheKey = (text: string, final: boolean) =>
  final ? `final:${text}` : `live:${text}`;

export function parseCached(
  text: string,
  opts: { final?: boolean } = {},
): XmlTree {
  const final = opts.final !== false;
  const key = cacheKey(text, final);
  const hit = PARSE_CACHE.get(key);

  if (hit !== undefined) return hit;

  const tree = parse(text, { final });
  PARSE_CACHE.set(key, tree);

  if (PARSE_CACHE.size > 50) {
    const first = PARSE_CACHE.keys().next();
    if (!first.done) PARSE_CACHE.delete(first.value);
  }

  return tree;
}
