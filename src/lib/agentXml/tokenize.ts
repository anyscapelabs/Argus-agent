// Pure scanner: it never rejects a tag it does not know. Hand-written single
// pass because `tokenize` must report "not closed yet" — a position, not a match.
import { isKnownTag } from "./schema";
import {
  Char,
  classifyAt,
  isAttrEndAt,
  isAttrNameCharAt,
  isSpaceAt,
  isTagNameCharAt,
  indexOfNewline,
  isTagNameStartAt,
  MAX_TAG_LEN,
  skipSpaces,
} from "./scan";

/**
 * `final` says whether more text can still arrive.
 *
 * Default `true` is right for stored messages: written means finished, so an
 * unterminated tag in them is prose. A live turn passes `false`, and a tag
 * still arriving is held back instead of shown as raw markup.
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

/**
 * Decode the HTML entities the backend emits when it escapes a value into an
 * attribute or a tag body.
 *
 * A scan, not three chained replacements: the text is walked once, and a `&`
 * that does not begin a well-formed entity is copied through without rebuilding
 * the string around it. Text with no entities returns the input unallocated.
 */
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
 * Whether a quoted attribute value ends at the quote just found.
 *
 * A quote only closes the value when a whole attribute name and `=` follows,
 * or the tag ends. `echo "hi" > f` has quotes that close nothing.
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
 * Where the tag ends, which is the `>` that closes it and not the first one in
 * sight. A terminal command is full of them — `2>&1`, `-gt`, `->`.
 *
 * Single pass with an explicit quote state so a `>` inside a quoted value cannot
 * end the tag. Interruptible by design: the caller must be able to tell "ran
 * out of input" from "tag invalid", which only a scanner holding its own
 * position can.
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

/** Index of the first space in `s`, or `-1`. */
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

  // Everything in front of `at` is complete, so the hold is safe: emit it as
  // text now, withhold only the fragment from `at` on.
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
      // Hold only what could still change. A tag never spans a line, so anything
      // before a newline is already final — holding prose there blanks the rest
      // of the reply for good.
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
      // A text token, not a buffer rewrite: rewriting `buf` would shift every offset
      // the scanner is holding.
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
