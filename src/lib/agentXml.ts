export type TagSchema = {
  tag: string;
  selfClosing: boolean;
  attributes: {
    name: string;
    required?: boolean;
    values?: readonly string[];
  }[];
};

export const TAG_SCHEMA: readonly TagSchema[] = [
  { tag: "bold", selfClosing: false, attributes: [] },
  { tag: "italic", selfClosing: false, attributes: [] },
  { tag: "underline", selfClosing: false, attributes: [] },
  { tag: "strikethrough", selfClosing: false, attributes: [] },
  { tag: "code", selfClosing: false, attributes: [] },
  { tag: "codeblock", selfClosing: false, attributes: [{ name: "language" }] },
  { tag: "link", selfClosing: false, attributes: [{ name: "href" }] },
  { tag: "h1", selfClosing: false, attributes: [] },
  { tag: "h2", selfClosing: false, attributes: [] },
  { tag: "h3", selfClosing: false, attributes: [] },
  { tag: "h4", selfClosing: false, attributes: [] },
  { tag: "table", selfClosing: false, attributes: [] },
  { tag: "tr", selfClosing: false, attributes: [] },
  { tag: "th", selfClosing: false, attributes: [] },
  { tag: "td", selfClosing: false, attributes: [] },
  {
    tag: "agent",
    selfClosing: false,
    attributes: [{ name: "id" }, { name: "name" }, { name: "state" }],
  },
  {
    tag: "agent-done",
    selfClosing: false,
    attributes: [{ name: "id" }, { name: "name" }, { name: "state" }],
  },
  { tag: "thinking", selfClosing: false, attributes: [{ name: "id" }] },
  { tag: "plan", selfClosing: false, attributes: [{ name: "id" }] },
  {
    tag: "step",
    selfClosing: false,
    attributes: [
      { name: "id" },
      { name: "status", values: ["done", "running", "pending"] },
    ],
  },
  {
    tag: "action",
    selfClosing: false,
    attributes: [
      { name: "id" },
      { name: "tool" },
      { name: "risk", values: ["low", "medium", "high"] },
      { name: "status", values: ["running", "success", "error"] },
    ],
  },
  {
    tag: "approval",
    selfClosing: false,
    attributes: [
      { name: "id" },
      {
        name: "type",
        values: [
          "git_push",
          "payment",
          "send_email",
          "delete",
          "login",
          "force_push",
        ],
      },
    ],
  },
  {
    tag: "diff",
    selfClosing: false,
    attributes: [{ name: "file" }, { name: "language" }],
  },
  {
    tag: "file",
    selfClosing: true,
    attributes: [
      { name: "path" },
      { name: "action", values: ["created", "edited", "read", "deleted"] },
      { name: "type" },
    ],
  },
  {
    tag: "document",
    selfClosing: true,
    attributes: [
      { name: "id" },
      { name: "path" },
      { name: "title" },
      {
        name: "doctype",
        values: ["docx", "pdf", "pptx", "xlsx", "csv", "md", "txt"],
      },
      { name: "pages" },
      { name: "status", values: ["ready", "generating"] },
    ],
  },
  {
    tag: "terminal",
    selfClosing: false,
    attributes: [
      { name: "id" },
      { name: "command" },
      { name: "status", values: ["running", "success", "error"] },
    ],
  },
  {
    tag: "sandbox",
    selfClosing: false,
    attributes: [
      { name: "command" },
      { name: "profile" },
      { name: "origin" },
      { name: "status" },
    ],
  },
  {
    tag: "check",
    selfClosing: true,
    attributes: [{ name: "status", values: ["pass", "retry"] }],
  },
  {
    tag: "email-draft",
    selfClosing: false,
    attributes: [{ name: "id" }, { name: "to" }, { name: "subject" }],
  },
  {
    tag: "browser-action",
    selfClosing: false,
    attributes: [{ name: "id" }, { name: "url" }, { name: "action" }],
  },
  {
    tag: "memory-ref",
    selfClosing: false,
    attributes: [{ name: "source" }, { name: "date" }],
  },
  {
    tag: "warning",
    selfClosing: false,
    attributes: [{ name: "severity", values: ["low", "medium", "high"] }],
  },
  {
    tag: "error",
    selfClosing: false,
    attributes: [{ name: "severity", values: ["low", "medium", "high"] }],
  },
] as const;

const TAG_MAP: Map<string, TagSchema> = new Map(
  TAG_SCHEMA.map((s) => [s.tag, s]),
);

export function isKnownTag(tag: string): boolean {
  return TAG_MAP.has(tag);
}

export function getTagSchema(tag: string): TagSchema | undefined {
  return TAG_MAP.get(tag);
}

export type Token =
  | { kind: "text"; value: string }
  | { kind: "open"; tag: string; attrs: Record<string, string> }
  | { kind: "close"; tag: string };

const ENTITY_MAP: Record<string, string> = {
  amp: "&",
  lt: "<",
  gt: ">",
  quot: '"',
  apos: "'",
};

function decodeEntities(str: string): string {
  return str.replace(/&([a-z]+);/g, (m, n: string) => ENTITY_MAP[n] ?? m);
}

const ATTR_NAME = /[a-zA-Z0-9_:.-]/;
const ATTR_END = /^\s*(?:[a-zA-Z_][a-zA-Z0-9_:-]*\s*=|\/?\s*$)/;

function parseAttributes(raw: string): Record<string, string> {
  const out: Record<string, string> = {};
  let i = 0;

  while (i < raw.length) {
    while (i < raw.length && /[\s/]/.test(raw[i])) {
      i++;
    }

    const start = i;

    while (i < raw.length && ATTR_NAME.test(raw[i])) {
      i++;
    }

    if (i === start) {
      i++;
      continue;
    }

    const name = raw.slice(start, i);

    while (i < raw.length && /\s/.test(raw[i])) {
      i++;
    }

    if (raw[i] !== "=") {
      continue;
    }

    i++;

    while (i < raw.length && /\s/.test(raw[i])) {
      i++;
    }

    const quote = raw[i];

    if (quote !== '"' && quote !== "'") {
      const v = i;

      while (i < raw.length && !/[\s"'<>`]/.test(raw[i])) {
        i++;
      }

      out[name] = decodeEntities(raw.slice(v, i));
      continue;
    }

    i++;

    const v = i;

    while (i < raw.length) {
      if (raw[i] !== quote) {
        i++;
        continue;
      }

      // A quote only ends the value when what follows is the next attribute
      // or the end of the tag. `echo "hi" > f` carries quotes that are not
      // the end of anything, and stopping at one truncates the command.
      if (ATTR_END.test(raw.slice(i + 1))) {
        break;
      }

      i++;
    }

    out[name] = decodeEntities(raw.slice(v, i));
    i++;
  }

  return out;
}

/// Where the tag ends, which is the `>` that closes it and not the first one
/// in sight. A terminal command is full of them — `2>&1`, `-gt`, `->` — and
/// cutting a tag at one drops the rest of the command into the chat as prose.
function findTagEnd(buf: string, from: number): number {
  let quote: string | null = null;

  for (let k = from; k < buf.length; k++) {
    const ch = buf[k];

    if (quote !== null) {
      if (ch === quote) {
        quote = null;
      }

      continue;
    }

    if (ch === '"' || ch === "'") {
      quote = ch;
      continue;
    }

    if (ch === ">") {
      return k;
    }
  }

  return -1;
}

export function tokenize(buf: string): Token[] {
  const toks: Token[] = [];
  let i = 0;
  let start = 0;

  const flush = (end: number) => {
    if (end <= start) return;
    toks.push({ kind: "text", value: decodeEntities(buf.slice(start, end)) });
  };

  while (i < buf.length) {
    if (buf[i] !== "<") {
      i++;
      continue;
    }

    const end = findTagEnd(buf, i + 1);
    if (end === -1) {
      i++;
      continue;
    }

    const raw = buf.slice(i + 1, end).trim();
    if (raw.length === 0) {
      i = end + 1;
      start = i;
      continue;
    }

    const isClose = raw.startsWith("/");
    const isSelf = raw.endsWith("/");
    const body = isClose ? raw.slice(1) : isSelf ? raw.slice(0, -1) : raw;
    const sp = body.search(/\s/);
    let tag = (sp === -1 ? body : body.slice(0, sp)).toLowerCase();
    const attrStr = sp === -1 ? "" : body.slice(sp + 1);

    const ALIAS: Record<string, string> = {
      strong: "bold",
      b: "bold",
      em: "italic",
      i: "italic",
      u: "underline",
      a: "link",
      h1: "h2",
    };
    tag = ALIAS[tag] ?? tag;

    if (
      tag === "p" ||
      tag === "div" ||
      tag === "span" ||
      tag === "command" ||
      tag === "output"
    ) {
      flush(i);
      start = end + 1;
      i = end + 1;
      continue;
    }

    if (tag === "br") {
      flush(i);
      buf += "\n";
      i = end + 1;
      start = i;
      continue;
    }

    if (!/^[a-z][a-z0-9-]*$/.test(tag)) {
      if (isClose && !isKnownTag(tag)) {
        flush(i);
        start = end + 1;
        toks.push({ kind: "close", tag: "" });
        i = end + 1;
        continue;
      }

      i++;
      continue;
    }

    if (!isKnownTag(tag)) {
      flush(i);
      start = end + 1;
      i = end + 1;
      continue;
    }

    flush(i);
    start = end + 1;

    if (isClose) {
      toks.push({ kind: "close", tag });
      i = end + 1;
      continue;
    }

    if (isSelf) {
      toks.push({ kind: "open", tag, attrs: parseAttributes(attrStr) });
      toks.push({ kind: "close", tag });
      i = end + 1;
      continue;
    }

    toks.push({ kind: "open", tag, attrs: parseAttributes(attrStr) });
    i = end + 1;
  }

  flush(i);
  return toks;
}

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

    const isList = (l: string) => /^(?:[-•*]|\d+\.)\s+/.test(l);
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

function inlineMd(s: string): string {
  let t = s;
  t = t.replace(/\*\*([^*]+)\*\*/g, "<bold>$1</bold>");
  t = t.replace(/__([^_]+)__/g, "<bold>$1</bold>");
  t = t.replace(/~~([^~]+)~~/g, "<strikethrough>$1</strikethrough>");
  t = t.replace(
    /(^|[\s(])\*([^*\s][^*]*?)\*(?=[\s).,!?;:]|$)/g,
    "$1<italic>$2</italic>",
  );
  t = t.replace(
    /(^|[\s(])_([^_\s][^_]*?)_(?=[\s).,!?;:]|$)/g,
    "$1<italic>$2</italic>",
  );
  t = t.replace(/`([^`]+)`/g, "<code>$1</code>");
  t = t.replace(
    /!\[([^\]]*)\]\(([^)\s]+)\)/g,
    (_m: string, alt: string, src: string) =>
      /^https?:\/\//.test(src)
        ? `<link href="${src}">${alt}</link>`
        : `<link>${alt}</link>`,
  );
  t = t.replace(
    /\[([^\]]+)\]\(([^)\s]+)\)/g,
    (_m: string, txt: string, href: string) =>
      /^https?:\/\//.test(href)
        ? `<link href="${href}">${txt}</link>`
        : `<link>${txt}</link>`,
  );
  return t;
}

// Markdown must never rewrite tag bodies: action JSON with backticks or
// terminal output starting with `#` would corrupt commands and records.
// Split each line into tag spans (kept raw) and prose spans (normalized).
function inlineOutside(line: string): string {
  return line
    .split(/(<[^<>]*>)/g)
    .map((seg, i) => (i % 2 === 1 ? seg : inlineMd(seg)))
    .join("");
}

// Component tags whose multi-line bodies stay raw. Tables and headings are
// prose-level and never counted; inline tags never span lines.
const DEPTH_TAGS =
  "thinking|plan|step|action|approval|diff|terminal|sandbox|email-draft|browser-action|memory-ref|warning|error|codeblock";
// Attribute-safe: quoted `>` (common in terminal commands) must not end the tag.
const TAG_ATTRS = `(?:"[^"]*"|'[^']*'|[^<>"'])*`;

const DEPTH_RE = new RegExp(`</?(?:${DEPTH_TAGS})\\b${TAG_ATTRS}/?>`, "g");

function depthDelta(line: string): number {
  let d = 0;
  let m: RegExpExecArray | null;
  DEPTH_RE.lastIndex = 0;
  while ((m = DEPTH_RE.exec(line)) !== null) {
    const t = m[0];
    if (t.startsWith("</")) d -= 1;
    else if (!t.endsWith("/>")) d += 1;
  }
  return d;
}

// Complete single-line blocks (`<action>{...}</action>`) are shielded so
// prose normalization never rewrites their bodies (backticks in commands
// would otherwise break arg parsing and hide the step hint).
const SINGLE_RE = new RegExp(`<(${DEPTH_TAGS})\\b${TAG_ATTRS}>.*?</\\1>`, "g");

function shieldLine(line: string): {
  text: string;
  restore: (s: string) => string;
} {
  const saved: string[] = [];
  const text = line.replace(
    SINGLE_RE,
    (m) => `\u0000${saved.push(m) - 1}\u0000`,
  );
  const restore = (s: string) =>
    s.replace(
      /\u0000(\d+)\u0000/g,
      (mm: string, n: string) => saved[Number(n)] ?? mm,
    );
  return { text, restore };
}

function escCode(s: string): string {
  return s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}

function normalizeMdLine(line: string): string {
  const { text: masked, restore } = shieldLine(line);
  let text = masked;
  if (/<\/?(?:ul|ol|li)[\s>/]/i.test(text)) {
    text = text
      .replace(/<\/?(?:ul|ol)[^<>]*>/gi, "")
      .replace(/<li[^<>]*>/gi, "- ")
      .replace(/<\/li>/gi, "");
    if (text.trim() === "") return "";
  }
  const h = text.match(/^(#{1,6})\s+(.*)$/);
  if (h !== null) {
    const tag = h[1].length <= 2 ? "h2" : "h3";
    return restore(`<${tag}>${inlineOutside(h[2])}</${tag}>`);
  }

  if (/^\s*(-{3,}|\*{3,}|_{3,})\s*$/.test(text)) return "";

  return restore(inlineOutside(text.replace(/^\s*>\s?/, "")));
}

function tableRow(line: string): string[] | null {
  const t = line.trim();
  if (!/^\|.*\|$/.test(t)) return null;
  return t
    .slice(1, -1)
    .split("|")
    .map((c) => c.trim());
}

function tryTable(
  lines: string[],
  i: number,
): { xml: string; next: number } | null {
  const head = tableRow(lines[i]);
  if (head === null || i + 1 >= lines.length) return null;

  const sep = lines[i + 1].trim();
  if (!sep.includes("-") || !/^\|?[\s:\-|]+\|?$/.test(sep)) return null;

  let xml =
    "<table><tr>" +
    head.map((c) => `<th>${inlineOutside(c)}</th>`).join("") +
    "</tr>";
  let j = i + 2;

  while (j < lines.length) {
    const cells = tableRow(lines[j]);
    if (cells === null || cells.length !== head.length) break;
    xml +=
      "<tr>" +
      cells.map((c) => `<td>${inlineOutside(c)}</td>`).join("") +
      "</tr>";
    j++;
  }

  return { xml: xml + "</table>", next: j };
}

function normalizeMd(src: string): string {
  const lines = src.split("\n");
  const out: string[] = [];
  let depth = 0;
  let inFence = false;
  let fenceLang = "";
  const fenceBody: string[] = [];

  const flushFence = (closed: boolean) => {
    if (!closed) {
      out.push("```" + (fenceLang ? fenceLang : ""));
      out.push(...fenceBody);
      return;
    }
    const lang = fenceLang ? ` language="${fenceLang}"` : "";
    out.push(
      `<codeblock${lang}>${fenceBody.map(escCode).join("\n")}</codeblock>`,
    );
  };

  let k = 0;
  let inTag = false;
  while (k < lines.length) {
    if (depth > 0) {
      out.push(lines[k]);
      depth = Math.max(0, depth + depthDelta(lines[k]));
      k++;
      continue;
    }

    const fence = lines[k].match(/^```([a-zA-Z0-9_-]*)[^\S\n]*$/);
    if (fence !== null) {
      if (!inFence) {
        inFence = true;
        fenceLang = fence[1];
        fenceBody.length = 0;
      } else {
        inFence = false;
        flushFence(true);
        fenceLang = "";
      }
      k++;
      continue;
    }

    if (inFence) {
      fenceBody.push(lines[k]);
      k++;
      continue;
    }

    // Markdown does not belong inside a tag. A multi-line terminal command
    // carries `#` comments and backticks, and rewriting those turns a shell
    // script into headings before anyone has parsed a single attribute.
    if (inTag) {
      out.push(lines[k]);
      if (findTagEnd(lines[k], 0) !== -1) {
        inTag = false;
      }

      k++;
      continue;
    }

    const line = lines[k];
    const lt = line.indexOf("<");

    if (lt !== -1 && findTagEnd(line, lt + 1) === -1) {
      inTag = true;
      out.push(line);
      k++;
      continue;
    }

    const d = depthDelta(line);

    if (d > 0) {
      out.push(line);
    } else {
      const t = tryTable(lines, k);
      if (t !== null) {
        out.push(t.xml);
        k = t.next;
        continue;
      }
      out.push(normalizeMdLine(line));
    }
    depth = Math.max(0, depth + d);
    k++;
  }

  if (inFence) flushFence(false);

  return out.join("\n");
}

export function parse(buf: string): XmlTree {
  return buildTree(tokenize(normalizeMd(buf)));
}

const PARSE_CACHE = new Map<string, XmlTree>();

export function parseCached(text: string): XmlTree {
  const hit = PARSE_CACHE.get(text);

  if (hit !== undefined) return hit;

  const tree = parse(text);
  PARSE_CACHE.set(text, tree);

  if (PARSE_CACHE.size > 50) {
    const first = PARSE_CACHE.keys().next();
    if (!first.done) PARSE_CACHE.delete(first.value);
  }

  return tree;
}
