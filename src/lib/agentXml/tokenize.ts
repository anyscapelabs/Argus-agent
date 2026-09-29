// Turning a reply into tokens: open tags, close tags, and the text between
// them. Nothing here knows what a tag *means* — that is the schema's job — so
// this file stays a pure scanner and never rejects a tag it does not recognise.
//
// Every loop here is a hand-written single pass over character codes. A regular
// expression cannot be used for the parts that matter: `tokenize` has to be
// able to stop at the end of a stream and report "this tag has not closed
// yet", which is a statement about a position, not about a match.
import { isKnownTag } from "./schema";
import {
  Char,
  classifyAt,
  isAttrEndAt,
  isAttrNameCharAt,
  isSpaceAt,
  isTagNameCharAt,
  isTagNameStartAt,
  MAX_TAG_LEN,
  skipSpaces,
} from "./scan";

/**
 * `final` says whether more text can still arrive.
 *
 * The default is `true`, which is right for everything read back from the
 * database: a message that has been written is finished, so an unterminated
 * tag in it is prose and belongs in the output. A live turn passes `false`, and
 * the one difference is that a tag still arriving is held back instead of
 * being shown as raw markup.
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
 * A scan rather than three chained replacements: the text is walked once, and
 * a `&` that does not begin a well-formed entity is copied through untouched
 * without the whole string being rebuilt around it. The common case in a reply
 * is text with no entities in it at all, and this returns the input unchanged
 * without allocating.
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

    // A code point outside the valid range would throw; the reference is
    // untrusted, so it is dropped rather than allowed to stop the render.
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
 * A quote only closes the value when what follows is the next attribute or the
 * end of the tag. `echo "hi" > f` carries quotes that are not the end of
 * anything, and stopping at one truncates the command.
 */
function quoteClosesValue(raw: string, after: number): boolean {
  const i = skipSpaces(raw, after, raw.length);

  if (i >= raw.length) return true;
  if (raw.charCodeAt(i) === 0x2f) return true; // `/` closes a self-closing tag
  if (!isAttrNameCharAt(raw, i)) return false;

  // It has to be a whole attribute name, then `=`, for this to be the close.
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

/** Why a scan for a tag's end stopped. The distinction is the whole point. */
export type TagScan =
  /** The `>` that closes the tag is at this index. */
  | { kind: "closed"; end: number }
  /**
   * No `>` arrived before the end of the input, or before a newline, or before
   * the length budget. On a finished document this is prose. On a stream it is
   * a tag that is still arriving, and the caller must hold it back rather than
   * show it.
   */
  | { kind: "unterminated" };

/**
 * Where the tag ends, which is the `>` that closes it and not the first one in
 * sight. A terminal command is full of them — `2>&1`, `-gt`, `->` — and cutting
 * a tag at one drops the rest of the command into the chat as prose.
 *
 * A single pass with an explicit quote state, so a `>` inside a quoted
 * attribute value cannot end the tag. There is no regular expression here
 * because this has to be interruptible: the caller needs to know that the scan
 * ran out of input rather than that the tag was invalid, and only a scanner
 * that is holding its own position can tell those apart.
 */
export function scanTagEnd(buf: string, from: number): TagScan {
  let quote = 0;
  const limit = Math.min(buf.length, from + MAX_TAG_LEN);

  for (let k = from; k < limit; k++) {
    const ch = buf.charCodeAt(k);

    // A newline ends the tag region. A tag never spans lines, and without
    // this a `>` many lines later would swallow everything between.
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

  /**
   * Give up on text from `at` to the end of the buffer without emitting it.
   * Used for a tag that is still arriving, so a half-arrived `<terminal` is
   * held rather than shown as markup. The next call sees a longer buffer, the
   * tag closes, and it is emitted then.
   */
  const hold = (at: number) => {
    start = at;
  };

  while (i < buf.length) {
    if (buf.charCodeAt(i) !== 0x3c) {
      i++;
      continue;
    }

    const next = buf.charCodeAt(i + 1);

    // A `<` only opens a tag when one could actually be there. Prose is full
    // of them — `if a < b and c > d`, a comparison — and each of those used to
    // be read as markup: the first rendered as `<bold>` with the words between
    // it eaten, the second swallowed a whole line into a garbage tag. A tag
    // has a name right after the `<`.
    if (!isTagNameStartAt(buf, i + 1) && next !== 0x2f) {
      i++;
      continue;
    }

    const scan = scanTagEnd(buf, i + 1);

    if (scan.kind === "unterminated") {
      // The frontier. On a finished document there is nothing more coming, so
      // an unterminated tag is prose and belongs in the text. On a stream it is
      // a tag whose closing `>` has not arrived yet, and showing the fragment
      // is what put `<terminal id="a1" command="cd ~ && ls` in the chat as
      // literal text. Hold it; the next parse of the longer buffer releases it.
      if (opts.final === false && isTagNameStartAt(buf, i + 1)) {
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
      // A line break is a text token, not a mutation of the buffer: rewriting
      // `buf` here would shift every offset the scanner is holding.
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
