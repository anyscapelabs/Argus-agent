// Turning a reply into tokens: open tags, close tags, and the text between
// them. Nothing here knows what a tag *means* — that is the schema's job — so
// this file stays a pure scanner and never rejects a tag it does not recognise.
import { isKnownTag } from "./schema";
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
  return str
    .replace(/&([a-z]+);/g, (m, n: string) => ENTITY_MAP[n] ?? m)
    .replace(/&#(\d+);/g, (_, n: string) => String.fromCodePoint(Number(n)))
    .replace(/&#x([0-9a-fA-F]+);/g, (_, n: string) =>
      String.fromCodePoint(parseInt(n, 16)),
    );
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
export function findTagEnd(buf: string, from: number): number {
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

    // A `<` only opens a tag when one could actually be there. Prose is full
    // of them — `if a < b and c > d`, a comparison, a half-typed tag arriving
    // on a stream — and each of those used to be read as markup: the first
    // rendered as `<bold>` with the words between it eaten, the second
    // swallowed a whole line into a garbage tag. A tag has a name right after
    // the `<`, and it lives on one line.
    if (!/[A-Za-z/]/.test(buf[i + 1] ?? "")) {
      i++;
      continue;
    }

    const end = findTagEnd(buf, i + 1);
    if (end === -1 || buf.slice(i + 1, end).includes("\n")) {
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
