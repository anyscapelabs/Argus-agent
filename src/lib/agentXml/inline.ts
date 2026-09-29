// Inline formatting, as a scanner.
//
// Markdown emphasis is a delimiter-matching problem, not a search problem:
// `*` opens an emphasis only if a matching `*` follows, and which one it
// matches is decided by the rule set, not by the first candidate. Expressing
// that with regular expressions needs a pattern per construct plus lookaround
// emulated with capture groups, and the result is a chain of passes that each
// rebuild the whole line and can match across a boundary the previous pass
// just created.
//
// A single pass with an explicit delimiter stack expresses the same rules
// directly, cannot match a construct that spans a tag boundary, and visits
// every character once.

import { scanTagEnd } from "./tokenize";
import {
  isAttrNameCharAt,
  isSpaceAt,
  isTagNameCharAt,
  isTagNameStartAt,
} from "./scan";

const STAR = 0x2a;
const UNDERSCORE = 0x5f;
const TILDE = 0x7e;
const BACKTICK = 0x60;
const LBRACKET = 0x5b;
const RBRACKET = 0x5d;
const LPAREN = 0x28;
const RPAREN = 0x29;
const BANG = 0x21;
const BACKSLASH = 0x5c;

/**
 * A matched delimiter pair. `to` is where the body ends — the index the closing
 * run starts at — and `next` is where scanning resumes, past the whole run.
 * They differ whenever the closer is two characters wide, which is the normal
 * case for bold.
 */
type Mark = { tag: string; from: number; to: number; next: number };

/**
 * Format the inline spans of one run of prose.
 *
 * `s` must already be free of tags: `inlineOutside` splits on tag boundaries
 * first, so this never sees markup and can never wrap one.
 */
function inlineMd(s: string): string {
  // A code span is literal to the end — no other construct applies inside it,
  // so it is copied across whole and never rescanned.
  let out = "";
  let plain = 0;
  let i = 0;

  while (i < s.length) {
    const c = s.charCodeAt(i);

    if (c === BACKSLASH) {
      i += 2;
      continue;
    }

    if (c === BACKTICK) {
      const end = closingRun(s, i, BACKTICK);
      if (end !== -1) {
        const body = s.slice(i + 1, end);
        out += esc(s.slice(plain, i)) + "<code>" + body + "</code>";
        i = end + 1;
        plain = i;
        continue;
      }
    }

    if (c === BANG && s.charCodeAt(i + 1) === LBRACKET) {
      const link = readLink(s, i + 1);
      if (link !== null) {
        out += esc(s.slice(plain, i)) + renderLink(link.alt, link.href, true);
        i = link.next;
        plain = i;
        continue;
      }
    }

    if (c === LBRACKET) {
      const link = readLink(s, i);
      if (link !== null) {
        out += esc(s.slice(plain, i)) + renderLink(link.text, link.href, false);
        i = link.next;
        plain = i;
        continue;
      }
    }

    if (c === STAR || c === UNDERSCORE || c === TILDE) {
      const mark = matchEmphasis(s, i, c);
      if (mark !== null) {
        out +=
          esc(s.slice(plain, i)) +
          `<${mark.tag}>` +
          inlineMd(s.slice(mark.from, mark.to)) +
          `</${mark.tag}>`;
        i = mark.next;
        plain = i;
        continue;
      }
    }

    i++;
  }

  out += esc(s.slice(plain));
  return out;
}

/** Escape the three characters that would otherwise read back as markup. */
function esc(s: string): string {
  let out = "";
  for (let i = 0; i < s.length; i++) {
    const c = s.charCodeAt(i);
    if (c === 0x3c) out += "&lt;";
    else if (c === 0x3e) out += "&gt;";
    else if (c === 0x26) out += "&amp;";
    else out += s[i];
  }
  return out;
}

/**
 * The index of the closing run of `ch`, or `-1`.
 *
 * A run of three or more backticks closes on a run of the same length, which
 * is what lets a fenced span contain a single backtick. Anything else closes on
 * the next single one.
 */
function closingRun(s: string, open: number, ch: number): number {
  let run = 1;
  while (open + run < s.length && s.charCodeAt(open + run) === ch) run++;

  for (let i = open + run; i < s.length; i++) {
    if (s.charCodeAt(i) !== ch) continue;

    let n = 1;
    while (i + n < s.length && s.charCodeAt(i + n) === ch) n++;

    if (n === run) return i;
    i += n - 1;
  }

  return -1;
}

/**
 * Whether the delimiter at `open` is emphasis, and where it closes.
 *
 * An opener must be followed by something that can open a span, and the closer
 * must be followed by something that cannot be part of a word — otherwise
 * `snake_case_name` and `a * b` would be turned into italics. The delimiter
 * also has to be the same one it opened with, so `_` cannot close a `*`.
 */
function matchEmphasis(s: string, open: number, ch: number): Mark | null {
  // A run of two or more `*` or `_` opens the strong form. `~` is different:
  // one tilde is not emphasis at all and two are a strikethrough, never bold.
  let run = 1;
  while (open + run < s.length && s.charCodeAt(open + run) === ch) run++;
  if (run > 3) return null;

  const strong = ch !== TILDE && run >= 2;
  const need = ch === TILDE ? 2 : strong ? 2 : 1;
  const mark = ch === TILDE ? "strikethrough" : strong ? "bold" : "italic";

  const after = open + run;
  if (after >= s.length) return null;
  // A run of spaces after the opener is not an opener at all.
  if (isSpaceAt(s, after)) return null;

  for (let i = after; i < s.length; i++) {
    if (s.charCodeAt(i) === BACKSLASH) {
      i++;
      continue;
    }
    if (s.charCodeAt(i) !== ch) continue;

    let closeRun = 1;
    while (i + closeRun < s.length && s.charCodeAt(i + closeRun) === ch)
      closeRun++;
    if (closeRun < need) continue;
    if (i === after) continue;

    // Left-flanking: the closer cannot be followed by a word character.
    const next = s.charCodeAt(i + closeRun);
    if (
      i + closeRun < s.length &&
      !isSpaceAt(s, i + closeRun) &&
      isWordChar(next)
    ) {
      continue;
    }
    // For `_`, a closer inside a word does not count.
    if (ch === UNDERSCORE && i > after && isWordChar(s.charCodeAt(i - 1))) {
      continue;
    }

    return { tag: mark, from: after, to: i, next: i + closeRun };
  }

  return null;
}

function isWordChar(code: number): boolean {
  return (
    (code >= 0x30 && code <= 0x39) ||
    (code >= 0x41 && code <= 0x5a) ||
    (code >= 0x61 && code <= 0x7a) ||
    code === 0x5f
  );
}

type Link = { text: string; alt: string; href: string; next: number };

/** `[text](href)`, or `null` when the brackets do not close into a link. */
function readLink(s: string, at: number): Link | null {
  const close = matchingBracket(s, at);
  if (close === -1) return null;
  if (s.charCodeAt(close + 1) !== LPAREN) return null;

  let end = close + 2;
  let depth = 1;
  while (end < s.length) {
    const c = s.charCodeAt(end);
    if (c === BACKSLASH) {
      end += 2;
      continue;
    }
    if (c === LPAREN) depth++;
    if (c === RPAREN) {
      depth--;
      if (depth === 0) break;
    }
    end++;
  }
  if (end >= s.length) return null;

  const text = s.slice(at + 1, close);
  // A title after the href is legal Markdown and is dropped, not rendered.
  const href = s.slice(close + 2, end).split(" ")[0];

  return { text, alt: text, href, next: end + 1 };
}

/** The `]` matching the `[` at `at`, honouring nesting and backslash escapes. */
function matchingBracket(s: string, at: number): number {
  let depth = 0;

  for (let i = at; i < s.length; i++) {
    const c = s.charCodeAt(i);
    if (c === BACKSLASH) {
      i++;
      continue;
    }
    if (c === LBRACKET) depth++;
    if (c === RBRACKET) {
      depth--;
      if (depth === 0) return i;
    }
  }

  return -1;
}

/**
 * A destination is only rendered as a link when it is one the app can open.
 * Anything else — a relative path, a `javascript:` URL — shows its text and
 * drops the target, so a model cannot put a live link in front of the user.
 */
function renderLink(text: string, href: string, isImage: boolean): string {
  const body = isImage ? "" : inlineMd(text);
  if (isImage) return "";

  return isHttpUrl(href)
    ? `<link href="${esc(href)}">${body}</link>`
    : `<link>${body}</link>`;
}

function isHttpUrl(href: string): boolean {
  const lower = href.slice(0, 8).toLowerCase();
  return lower.startsWith("http://") || lower.startsWith("https://");
}

/**
 * Markdown must never rewrite tag bodies: action JSON with backticks or
 * terminal output starting with `#` would corrupt commands and records.
 *
 * The line is walked once, and each span is either a tag — copied across
 * untouched — or prose, which is formatted. Splitting with a regular
 * expression meant building a match array and a map for every line, and could
 * not tell a tag that had not closed from prose containing `<`.
 */
export function inlineOutside(line: string): string {
  let out = "";
  let plain = 0;
  let i = 0;

  while (i < line.length) {
    if (line.charCodeAt(i) !== 0x3c) {
      i++;
      continue;
    }

    const scan = scanTagEnd(line, i + 1);
    if (scan.kind === "unterminated") {
      // No `>` before the end of the line: a comparison, not a tag.
      i++;
      continue;
    }

    if (!isTagRegion(line, i + 1, scan.end)) {
      i++;
      continue;
    }

    out += inlineMd(line.slice(plain, i)) + line.slice(i, scan.end + 1);
    i = scan.end + 1;
    plain = i;
  }

  return out + inlineMd(line.slice(plain));
}

/**
 * Whether the text between `<` and `>` is a tag rather than an angle-bracketed
 * fragment of prose.
 *
 * Every well-formed tag is passed through untouched, not a chosen few: the
 * caller decides what a tag means, and this function's only job is to keep
 * Markdown from rewriting one. Whitelisting here would silently drop `<step>`
 * or `<terminal>` and leave their bodies as raw text in the chat.
 */
function isTagRegion(line: string, from: number, to: number): boolean {
  let i = from;

  if (line.charCodeAt(i) === 0x2f)
    i++; // a close tag
  else if (line.charCodeAt(i) === 0x21) return false; // a comment or CDATA

  if (i >= to || !isTagNameStartAt(line, i)) return false;
  i++;
  while (i < to && isTagNameCharAt(line, i)) i++;

  // Attributes may follow, and a `/` may close a self-closing tag. The scan
  // already proved the region is quote-balanced, so walking it here is enough.
  while (i < to) {
    const c = line.charCodeAt(i);
    if (isSpaceAt(line, i) || c === 0x2f) {
      i++;
      continue;
    }
    if (c === 0x3d) {
      // `name="value"`: the name, the `=`, then a quoted value.
      i++;
      while (i < to && isSpaceAt(line, i)) i++;
      if (
        i < to &&
        (line.charCodeAt(i) === 0x22 || line.charCodeAt(i) === 0x27)
      ) {
        const quote = line.charCodeAt(i);
        i++;
        while (i < to && line.charCodeAt(i) !== quote) i++;
        if (i < to) i++;
        continue;
      }
      // An unquoted value runs to the next space.
      while (i < to && !isSpaceAt(line, i)) i++;
      continue;
    }
    if (isAttrNameCharAt(line, i)) {
      while (i < to && isAttrNameCharAt(line, i)) i++;
      continue;
    }
    return false;
  }

  return true;
}
