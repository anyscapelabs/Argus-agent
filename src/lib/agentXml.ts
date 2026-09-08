export type TagSchema = {
  tag: string;
  selfClosing: boolean;
  attributes: { name: string; required?: boolean; values?: readonly string[] }[];
};

export const TAG_SCHEMA: readonly TagSchema[] = [
  { tag: "bold", selfClosing: false, attributes: [] },
  { tag: "italic", selfClosing: false, attributes: [] },
  { tag: "underline", selfClosing: false, attributes: [] },
  { tag: "strikethrough", selfClosing: false, attributes: [] },
  { tag: "code", selfClosing: false, attributes: [] },
  { tag: "link", selfClosing: false, attributes: [{ name: "href" }] },
  { tag: "h1", selfClosing: false, attributes: [] },
  { tag: "h2", selfClosing: false, attributes: [] },
  { tag: "h3", selfClosing: false, attributes: [] },
  { tag: "h4", selfClosing: false, attributes: [] },
  { tag: "table", selfClosing: false, attributes: [] },
  { tag: "tr", selfClosing: false, attributes: [] },
  { tag: "th", selfClosing: false, attributes: [] },
  { tag: "td", selfClosing: false, attributes: [] },
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
      { name: "type", values: ["git_push", "payment", "send_email", "delete", "login", "force_push"] },
    ],
  },
  { tag: "diff", selfClosing: false, attributes: [{ name: "file" }, { name: "language" }] },
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
      { name: "path" },
      { name: "title" },
      { name: "doctype", values: ["docx", "pdf"] },
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
  { tag: "email-draft", selfClosing: false, attributes: [{ name: "id" }, { name: "to" }, { name: "subject" }] },
  { tag: "browser-action", selfClosing: false, attributes: [{ name: "id" }, { name: "url" }, { name: "action" }] },
  { tag: "memory-ref", selfClosing: false, attributes: [{ name: "source" }, { name: "date" }] },
  { tag: "warning", selfClosing: false, attributes: [{ name: "severity", values: ["low", "medium", "high"] }] },
  { tag: "error", selfClosing: false, attributes: [{ name: "severity", values: ["low", "medium", "high"] }] },
] as const;

const tagMap: Map<string, TagSchema> = new Map(TAG_SCHEMA.map((s) => [s.tag, s]));

export function isKnownTag(tag: string): boolean {
  return tagMap.has(tag);
}

export function getTagSchema(tag: string): TagSchema | undefined {
  return tagMap.get(tag);
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

function parseAttributes(raw: string): Record<string, string> {
  const out: Record<string, string> = {};
  const re = /([a-zA-Z_][a-zA-Z0-9_:-]*)\s*=\s*("([^"]*)"|'([^']*)'|([^\s"'<>`]+))/gs;
  let m: RegExpExecArray | null;
  while ((m = re.exec(raw)) !== null) {
    const k = m[1];
    const v = m[3] ?? m[4] ?? m[5] ?? "";
    out[k] = decodeEntities(v);
  }
  return out;
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

    const end = buf.indexOf(">", i + 1);
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
    const tag = (sp === -1 ? body : body.slice(0, sp)).toLowerCase();
    const attrStr = sp === -1 ? "" : body.slice(sp + 1);

    if (!/^[a-z][a-z0-9-]*$/.test(tag)) {
      i++;
      continue;
    }

    if (!isKnownTag(tag)) {
      i++;
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

export function buildTree(toks: Token[]): XmlTree {
  const blks: BlockNode[] = [];
  let buf = "";
  let cur: BlockNode | null = null;
  const inline = new Set(["bold", "italic", "underline", "strikethrough", "code", "link"]);
  const tableInner = new Set(["tr", "th", "td"]);

  const flush = () => {
    const v = buf.trim();
    buf = "";
    if (v.length === 0) return;
    // Each line is its own paragraph; consecutive list lines stay grouped so
    // the renderer can build ul/ol out of them.
    const isList = (l: string) => /^(?:[-•*]|\d+\.)\s+/.test(l);
    let group: string[] = [];
    const pushGroup = () => {
      if (group.length === 0) return;
      blks.push({ kind: "paragraph", tag: "p", attrs: {}, children: [{ kind: "text", value: group.join("\n") }] });
      group = [];
    };
    for (const raw of v.split("\n")) {
      const line = raw.trim();
      if (line.length === 0) continue;
      if (isList(line)) {
        group.push(line);
      } else {
        pushGroup();
        blks.push({ kind: "paragraph", tag: "p", attrs: {}, children: [{ kind: "text", value: line }] });
      }
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
      if (cur && (cur.tag === "table" || cur.tag === "tr") && tableInner.has(tag)) {
        const raw = `<${tag}>`;
        cur.children.push({ kind: "text", value: raw });
        continue;
      }
      flush();
      const kind: BlockNode["kind"] =
        tag === "h1" || tag === "h2" || tag === "h3" || tag === "h4" ? "heading" : "component";
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
    if (cur && (cur.tag === "table" || cur.tag === "tr") && tableInner.has(tag)) {
      const raw = `</${tag}>`;
      cur.children.push({ kind: "text", value: raw });
      continue;
    }

    if (!cur) continue;
    if (cur.tag !== tag) continue;
    buf = "";
    cur = null;
  }

  flush();

  return blks.filter((blk) => {
    if (blk.kind !== "paragraph") return true;
    return blk.children.some((c) => c.value.trim().length > 0);
  });
}

// Markdown leaks from every model sooner or later; convert the common cases
// to the dialect instead of showing literal ** junk. Fence content passes
// through untouched.
function inlineMd(s: string): string {
  let t = s;
  t = t.replace(/\*\*([^*]+)\*\*/g, "<bold>$1</bold>");
  t = t.replace(/__([^_]+)__/g, "<bold>$1</bold>");
  t = t.replace(/(^|[\s(])\*([^*\s][^*]*?)\*(?=[\s).,!?;:]|$)/g, "$1<italic>$2</italic>");
  t = t.replace(/(^|[\s(])_([^_\s][^_]*?)_(?=[\s).,!?;:]|$)/g, "$1<italic>$2</italic>");
  t = t.replace(/`([^`]+)`/g, "<code>$1</code>");
  t = t.replace(/\[([^\]]+)\]\((https?:\/\/[^)\s]+)\)/g, '<link href="$2">$1</link>');
  return t;
}

function normalizeMdLine(line: string): string {
  const h = line.match(/^(#{1,6})\s+(.*)$/);
  if (h !== null) {
    const tag = h[1].length <= 2 ? "h2" : "h3";
    return `<${tag}>${inlineMd(h[2])}</${tag}>`;
  }
  if (/^\s*(-{3,}|\*{3,}|_{3,})\s*$/.test(line)) return ""; // hr junk, drop it
  return inlineMd(line.replace(/^\s*>\s?/, ""));
}

function normalizeMd(src: string): string {
  const parts = src.split(/```[a-zA-Z0-9_-]*[^\S\n]*\n?/);
  return parts
    .map((seg, i) => {
      if (i % 2 === 1) return seg; // fence content: leave as-is
      return seg.split("\n").map(normalizeMdLine).join("\n");
    })
    .join("\n");
}

export function parse(buf: string): XmlTree {
  return buildTree(tokenize(normalizeMd(buf)));
}
