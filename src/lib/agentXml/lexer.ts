// Lexer: char table + tag scanning + tokenize. One pass, resumable for
// streaming: `tokenize` reports "not closed yet" as a position, not a match.
const ASCII_LAST = 0x7f;

/** Bit flags, so one table can answer several questions with one read. */
export const enum Char {
  None = 0,
  Space = 1 << 0,
  TagNameStart = 1 << 1,
  TagName = 1 << 2,
  AttrName = 1 << 3,
  Digit = 1 << 4,
  HexDigit = 1 << 5,
  Alpha = 1 << 6,
  /** Terminates an unquoted attribute value. */
  AttrEnd = 1 << 7,
}

function build(): Uint8Array {
  const t = new Uint8Array(ASCII_LAST + 1);

  const add = (lo: number, hi: number, flag: Char) => {
    for (let c = lo; c <= hi; c++) t[c] |= flag;
  };

  add(0x09, 0x0d, Char.Space);
  add(0x20, 0x20, Char.Space);
  add(0x28, 0x2b, Char.AttrEnd);
  add(0x2d, 0x2d, Char.TagName | Char.AttrName);
  // Digits are name chars too: `h2`, `h3`, `email-draft`.
  add(
    0x30,
    0x39,
    Char.Digit | Char.HexDigit | Char.TagName | Char.AttrName | Char.AttrEnd,
  );
  add(0x3a, 0x3a, Char.TagName | Char.AttrName | Char.AttrEnd);
  add(0x3b, 0x3b, Char.AttrEnd);
  add(0x3c, 0x3e, Char.AttrEnd);
  add(0x3f, 0x3f, Char.TagName | Char.AttrName | Char.AttrEnd);
  add(
    0x41,
    0x5a,
    Char.TagName |
      Char.TagNameStart |
      Char.AttrName |
      Char.HexDigit |
      Char.Alpha |
      Char.AttrEnd,
  );
  add(0x5b, 0x5b, Char.AttrEnd);
  add(0x5d, 0x5d, Char.AttrEnd);
  add(0x5f, 0x5f, Char.TagName | Char.AttrName | Char.AttrEnd);
  add(
    0x61,
    0x7a,
    Char.TagName |
      Char.TagNameStart |
      Char.AttrName |
      Char.HexDigit |
      Char.Alpha |
      Char.AttrEnd,
  );

  return t;
}

const TABLE = build();

/** `Char.None` outside ASCII: a `<` followed by `π` or `≤` is prose. */
export function classify(code: number): Char {
  return code <= ASCII_LAST ? TABLE[code] : Char.None;
}

export function classifyAt(s: string, i: number): Char {
  return classify(s.charCodeAt(i));
}

export function isSpaceAt(s: string, i: number): boolean {
  return (classifyAt(s, i) & Char.Space) !== 0;
}

export function isTagNameStartAt(s: string, i: number): boolean {
  return (classifyAt(s, i) & Char.TagNameStart) !== 0;
}

export function isTagNameCharAt(s: string, i: number): boolean {
  return (classifyAt(s, i) & Char.TagName) !== 0;
}

export function isAttrNameCharAt(s: string, i: number): boolean {
  return (classifyAt(s, i) & Char.AttrName) !== 0;
}

export function isAttrEndAt(s: string, i: number): boolean {
  return (classifyAt(s, i) & Char.AttrEnd) !== 0;
}

export function skipSpaces(s: string, from: number, limit: number): number {
  let i = from;
  while (i < limit && isSpaceAt(s, i)) i++;
  return i;
}

/** Index of the first newline in `s[from..limit)`, or `-1`. */
export function indexOfNewline(s: string, from: number, limit: number): number {
  for (let i = from; i < limit; i++) {
    if (s.charCodeAt(i) === 0x0a) return i;
  }
  return -1;
}

// A `<` unclosed past this budget is prose, not a tag still arriving. Without
// the bound one stray `<` holds back the rest of the message.
export const MAX_TAG_LEN = 4096;

import { isKnownTag } from "./schema";

/**
 * `final: false` on a live turn holds an unterminated tag back instead of
 * showing it as raw markup. Stored messages are finished, so `true`.
 */
export type TokenizeOpts = { final?: boolean };

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

const AMP = 0x26;
const HASH = 0x23;
const LOWER_X = 0x78;
const UPPER_X = 0x58;

/** Decode the HTML entities the backend emits when it escapes a value. */
function decodeEntities(str: string): string {
  const amp = str.indexOf("&");
  if (amp === -1) return str;

  let out = "";
  let copied = 0;
  let i = amp;

  while (i < str.length) {
    if (str.charCodeAt(i) !== AMP) {
      i++;
      continue;
    }

    const end = str.indexOf(";", i + 1);
    if (end === -1 || end === i + 1) {
      i++;
      continue;
    }

    const decoded = decodeOneEntity(str, i + 1, end);
    if (decoded === null) {
      i++;
      continue;
    }

    out += str.slice(copied, i) + decoded;
    i = end + 1;
    copied = i;
  }

  return copied === 0 ? str : out + str.slice(copied);
}

/** The text between `&` and `;`, already positioned, or `null` if not one. */
function decodeOneEntity(
  str: string,
  from: number,
  end: number,
): string | null {
  if (str.charCodeAt(from) === HASH) {
    let k = from + 1;
    let radix = 10;

    if (
      k < end &&
      (str.charCodeAt(k) === LOWER_X || str.charCodeAt(k) === UPPER_X)
    ) {
      radix = 16;
      k++;
    }

    if (k >= end) return null;

    let value = 0;
    for (; k < end; k++) {
      const cls = classifyAt(str, k);
      if (cls & Char.HexDigit) {
        const c = str.charCodeAt(k);
        const digit = c <= 0x39 ? c - 0x30 : (c | 0x20) - 0x57;
        value = value * radix + digit;
        continue;
      }
      if (radix === 10 && cls & Char.Digit) {
        value = value * 10 + (c2(str, k) - 0x30);
        continue;
      }
      return null;
    }

    // Out-of-range would throw; untrusted ref, drop it.
    if (value < 0 || value > 0x10ffff) return null;

    try {
      return String.fromCodePoint(value);
    } catch {
      return null;
    }
  }

  const name = str.slice(from, end);
  if (name.length > 8) return null;

  for (let k = 0; k < name.length; k++) {
    const c = name.charCodeAt(k);
    const lower = c >= 0x41 && c <= 0x5a ? c + 0x20 : c;
    if (lower < 0x61 || lower > 0x7a) return null;
  }

  return ENTITY_MAP[name] ?? null;
}

function c2(s: string, i: number): number {
  return s.charCodeAt(i);
}

/**
 * A quote only closes the value when a whole attribute name and `=` follows, or
 * the tag ends. `echo "hi" > f` has quotes that close nothing.
 */
function quoteClosesValue(raw: string, after: number): boolean {
  const i = skipSpaces(raw, after, raw.length);

  if (i >= raw.length) return true;
  if (raw.charCodeAt(i) === 0x2f) return true; // `/` closes a self-closing tag
  if (!isAttrNameCharAt(raw, i)) return false;

  let k = i;
  while (k < raw.length && isAttrNameCharAt(raw, k)) k++;
  k = skipSpaces(raw, k, raw.length);

  return k < raw.length && raw.charCodeAt(k) === 0x3d;
}

function parseAttributes(raw: string): Record<string, string> {
  const out: Record<string, string> = {};
  let i = 0;

  while (i < raw.length) {
    while (
      i < raw.length &&
      (isSpaceAt(raw, i) || raw.charCodeAt(i) === 0x2f)
    ) {
      i++;
    }

    const start = i;

    while (i < raw.length && isAttrNameCharAt(raw, i)) {
      i++;
    }

    if (i === start) {
      i++;
      continue;
    }

    const name = raw.slice(start, i);

    i = skipSpaces(raw, i, raw.length);

    if (raw.charCodeAt(i) !== 0x3d) {
      continue;
    }

    i++;

    i = skipSpaces(raw, i, raw.length);

    const quote = raw.charCodeAt(i);

    if (quote !== 0x22 && quote !== 0x27) {
      const v = i;

      while (i < raw.length && !isAttrEndAt(raw, i) && !isSpaceAt(raw, i)) {
        i++;
      }

      out[name] = decodeEntities(raw.slice(v, i));
      continue;
    }

    i++;

    const v = i;

    while (i < raw.length) {
      if (raw.charCodeAt(i) !== quote) {
        i++;
        continue;
      }

      if (quoteClosesValue(raw, i + 1)) {
        break;
      }

      i++;
    }

    out[name] = decodeEntities(raw.slice(v, i));
    i++;
  }

  return out;
}

/** Why a scan for a tag's end stopped. */
export type TagScan =
  /** The `>` that closes the tag is at this index. */
  | { kind: "closed"; end: number }
  /**
   * No `>` before end of input, newline, or the length budget. Prose on a
   * finished document; on a stream, hold it back.
   */
  | { kind: "unterminated" };

/**
 * The `>` that closes the tag, not the first one in sight — a terminal command
 * is full of them. Quote-aware, so `command="ls > f"` does not end early.
 */
export function scanTagEnd(buf: string, from: number): TagScan {
  let quote = 0;
  const limit = Math.min(buf.length, from + MAX_TAG_LEN);

  for (let k = from; k < limit; k++) {
    const ch = buf.charCodeAt(k);

    // A tag never spans lines; without this a later `>` swallows everything.
    if (ch === 0x0a) return { kind: "unterminated" };

    if (quote !== 0) {
      if (ch === quote) quote = 0;
      continue;
    }

    if (ch === 0x22 || ch === 0x27) {
      quote = ch;
      continue;
    }

    if (ch === 0x3e) return { kind: "closed", end: k };
  }

  return { kind: "unterminated" };
}

/** The index of a tag's closing `>`, or `-1`. For callers that do not care why. */
export function findTagEnd(buf: string, from: number): number {
  const r = scanTagEnd(buf, from);
  return r.kind === "closed" ? r.end : -1;
}

/** Whether `tag` is `[a-z][a-z0-9-]*`, the only shape a tag name may take. */
function isWellFormedTagName(tag: string): boolean {
  if (tag.length === 0 || !isTagNameStartAt(tag, 0)) return false;
  for (let k = 1; k < tag.length; k++) {
    if (!isTagNameCharAt(tag, k)) return false;
  }
  return true;
}

function firstSpace(s: string): number {
  for (let k = 0; k < s.length; k++) {
    if (isSpaceAt(s, k)) return k;
  }
  return -1;
}

export function tokenize(buf: string, opts: TokenizeOpts = {}): Token[] {
  const toks: Token[] = [];
  let i = 0;
  let start = 0;

  const flush = (end: number) => {
    if (end <= start) return;
    toks.push({ kind: "text", value: decodeEntities(buf.slice(start, end)) });
  };

  const hold = (at: number) => {
    flush(at);
    start = at;
  };

  while (i < buf.length) {
    if (buf.charCodeAt(i) !== 0x3c) {
      i++;
      continue;
    }

    const next = buf.charCodeAt(i + 1);

    // Prose is full of `<` — `if a < b and c > d`. A tag has a name after it.
    if (!isTagNameStartAt(buf, i + 1) && next !== 0x2f) {
      i++;
      continue;
    }

    const scan = scanTagEnd(buf, i + 1);

    if (scan.kind === "unterminated") {
      // Hold only what could still change. A tag never spans a line, so holding
      // prose there blanks the rest of the reply for good.
      if (
        opts.final === false &&
        isTagNameStartAt(buf, i + 1) &&
        indexOfNewline(buf, i, buf.length) === -1
      ) {
        hold(i);
        return toks;
      }
      i++;
      continue;
    }

    const end = scan.end;
    const raw = buf.slice(i + 1, end).trim();
    if (raw.length === 0) {
      i = end + 1;
      start = i;
      continue;
    }

    const isClose = raw.charCodeAt(0) === 0x2f;
    const isSelf = raw.charCodeAt(raw.length - 1) === 0x2f;
    const body = isClose ? raw.slice(1) : isSelf ? raw.slice(0, -1) : raw;
    const sp = firstSpace(body);
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
      // A text token, not a buffer rewrite: rewriting shifts every offset the
      // scanner holds.
      if (i > start) {
        toks.push({ kind: "text", value: decodeEntities(buf.slice(start, i)) });
      }
      toks.push({ kind: "text", value: "\n" });
      i = end + 1;
      start = i;
      continue;
    }

    if (!isWellFormedTagName(tag)) {
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
