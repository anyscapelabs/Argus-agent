// Markdown: inline emphasis/links (inlineOutside) + block normalization (normalizeMd). Single home for prose-to-XML so AgentBubble has one way in.
import { scanTagEnd } from "./lexer";
import {
  Char,
  classifyAt,
  isAttrNameCharAt,
  isSpaceAt,
  isTagNameCharAt,
  isTagNameStartAt,
} from "./lexer";
// Emphasis is delimiter matching, not search: a run opens only if a matching
// closer follows, so a chain of regex passes can never match across a tag here.


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

/** `next` is past the closing run, `to` is its start. */
type Mark = { tag: string; from: number; to: number; next: number };

/** Format the inline spans of one run of tag-free prose. */
function inlineMd(s: string): string {
  // A code span is literal to the end, so copy it across whole.
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

/** Index of the closing run of `ch`, or `-1`. A run closes on one of equal length. */
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
 * Whether the delimiter at `open` is emphasis, and where it closes. Without the
 * flanking checks `snake_case_name` and `a * b` turn italic.
 */
function matchEmphasis(s: string, open: number, ch: number): Mark | null {
  // 2+ `*`/`_` is strong. `~` is never bold: one is not emphasis, two strike.
  let run = 1;
  while (open + run < s.length && s.charCodeAt(open + run) === ch) run++;
  if (run > 3) return null;

  const strong = ch !== TILDE && run >= 2;
  const need = ch === TILDE ? 2 : strong ? 2 : 1;
  const mark = ch === TILDE ? "strikethrough" : strong ? "bold" : "italic";

  const after = open + run;
  if (after >= s.length) return null;
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
  // A title after the href is legal Markdown; drop it.
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
 * Only http(s) destinations render as links. A relative path or a `javascript:`
 * URL shows its text and drops the target, so a model cannot put a live link in
 * front of the user.
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
 * Markdown must never rewrite tag bodies: backticks in action JSON would
 * corrupt commands. Each span is a tag, copied across, or prose, which is
 * formatted.
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
      // No `>` before end of line: a comparison, not a tag.
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
 * Whether the text between `<` and `>` is a tag rather than prose. No whitelist:
 * the caller decides what a tag means, and dropping one leaves its body as raw
 * text.
 */
function isTagRegion(line: string, from: number, to: number): boolean {
  let i = from;

  if (line.charCodeAt(i) === 0x2f)
    i++; // a close tag
  else if (line.charCodeAt(i) === 0x21) return false; // a comment or CDATA

  if (i >= to || !isTagNameStartAt(line, i)) return false;
  i++;
  while (i < to && isTagNameCharAt(line, i)) i++;

  // The scan already proved the region quote-balanced, so walking it is enough.
  while (i < to) {
    const c = line.charCodeAt(i);
    if (isSpaceAt(line, i) || c === 0x2f) {
      i++;
      continue;
    }
    if (c === 0x3d) {
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
      // Unquoted value runs to the next space.
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

// Terminal and diff stay raw: a shell script's `#` is a comment, not a heading.
const PROSE_TAGS = new Set(["thinking", "plan", "step", "warning", "error"]);

const PAYLOAD_TAGS = new Set([
  "action",
  "approval",
  "diff",
  "terminal",
  "sandbox",
  "email-draft",
  "browser-action",
  "memory-ref",
  "codeblock",
]);

const SPANNING_TAGS = new Set([...PROSE_TAGS, ...PAYLOAD_TAGS]);

type Span = { name: string; close: boolean; self: boolean; end: number };

// Must agree with scanTagEnd, or a line goes half-raw.
function scanSpans(line: string): Span[] {
  const out: Span[] = [];
  let i = 0;

  while (i < line.length) {
    if (line.charCodeAt(i) !== 0x3c) {
      i++;
      continue;
    }

    const scan = scanTagEnd(line, i + 1);
    if (scan.kind === "unterminated") {
      i++;
      continue;
    }

    let k = i + 1;
    const close = line.charCodeAt(k) === 0x2f;
    if (close) k++;

    const nameStart = k;
    while (k < scan.end && isTagNameCharAt(line, k)) k++;
    const name = line.slice(nameStart, k).toLowerCase();

    let after = k;
    while (after < scan.end && isSpaceAt(line, after)) after++;
    const self = line.charCodeAt(scan.end - 1) === 0x2f;

    if (name.length > 0 && SPANNING_TAGS.has(name)) {
      out.push({ name, close, self, end: scan.end + 1 });
    }

    i = scan.end + 1;
  }

  return out;
}

function depthDelta(line: string): number {
  let d = 0;
  for (const s of scanSpans(line)) {
    if (s.close) d -= 1;
    else if (!s.self) d += 1;
  }
  return d;
}

function trackTags(stack: string[], line: string): string[] {
  const next = [...stack];
  for (const s of scanSpans(line)) {
    if (s.close) {
      next.pop();
    } else if (!s.self) {
      next.push(s.name);
    }
  }
  return next;
}

function readFence(line: string): string | null {
  if (
    line.charCodeAt(0) !== 0x60 ||
    line.charCodeAt(1) !== 0x60 ||
    line.charCodeAt(2) !== 0x60
  ) {
    return null;
  }

  let k = 3;
  const start = k;
  while (k < line.length) {
    const cls = classifyAt(line, k);
    if (cls & (Char.TagName | Char.Digit | Char.AttrName)) {
      k++;
      continue;
    }
    break;
  }
  const lang = line.slice(start, k);

  // Nothing but spaces may follow, else it is prose.
  while (k < line.length) {
    if (isSpaceAt(line, k) && line.charCodeAt(k) !== 0x0a) {
      k++;
      continue;
    }
    return null;
  }

  return lang;
}

// Mask payload blocks so prose normalization never rewrites tool JSON.
// Multi-line payloads ride the depth tracker instead.
function shieldLine(line: string): {
  text: string;
  restore: (s: string) => string;
} {
  const saved: string[] = [];
  let out = "";
  let copied = 0;
  let i = 0;

  while (i < line.length) {
    if (line.charCodeAt(i) !== 0x3c) {
      i++;
      continue;
    }

    const scan = scanTagEnd(line, i + 1);
    if (scan.kind === "unterminated") {
      i++;
      continue;
    }

    const open = readTagName(line, i + 1, scan.end);
    if (
      open === null ||
      open.close ||
      open.self ||
      !PAYLOAD_TAGS.has(open.name)
    ) {
      i = scan.end + 1;
      continue;
    }

    const closeTag = `</${open.name}>`;
    const bodyEnd = line.indexOf(closeTag, scan.end + 1);

    if (bodyEnd === -1) {
      i = scan.end + 1;
      continue;
    }

    const stop = bodyEnd + closeTag.length;
    out +=
      line.slice(copied, i) +
      `\u0000${saved.push(line.slice(i, stop)) - 1}\u0000`;
    i = stop;
    copied = stop;
  }

  if (copied === 0) return { text: line, restore: (s) => s };

  out += line.slice(copied);

  return { text: out, restore: (s) => restorePlaceholders(s, saved) };
}

function readTagName(
  line: string,
  from: number,
  to: number,
): { name: string; close: boolean; self: boolean } | null {
  let k = from;
  const close = line.charCodeAt(k) === 0x2f;
  if (close) k++;

  const start = k;
  while (k < to && isTagNameCharAt(line, k)) k++;
  if (k === start) return null;

  let after = k;
  while (after < to && isSpaceAt(line, after)) after++;

  return {
    name: line.slice(start, k).toLowerCase(),
    close,
    self: line.charCodeAt(to - 1) === 0x2f,
  };
}

function restorePlaceholders(s: string, saved: string[]): string {
  let at = s.indexOf("\u0000");
  if (at === -1) return s;

  let out = "";
  let copied = 0;
  let i = at;

  while (i < s.length) {
    if (s.charCodeAt(i) !== 0) {
      i++;
      continue;
    }
    const end = s.indexOf("\u0000", i + 1);
    if (end === -1) break;
    const n = Number(s.slice(i + 1, end));
    out += s.slice(copied, i) + (saved[n] ?? s.slice(i, end + 1));
    i = end + 1;
    copied = i;
  }

  return copied === 0 ? s : out + s.slice(copied);
}

function escCode(s: string): string {
  let at = -1;
  for (let i = 0; i < s.length; i++) {
    const c = s.charCodeAt(i);
    if (c === 0x26 || c === 0x3c || c === 0x3e) {
      at = i;
      break;
    }
  }
  if (at === -1) return s;

  let out = s.slice(0, at);
  for (let i = at; i < s.length; i++) {
    const c = s.charCodeAt(i);
    if (c === 0x26) out += "&amp;";
    else if (c === 0x3c) out += "&lt;";
    else if (c === 0x3e) out += "&gt;";
    else out += s[i];
  }
  return out;
}

// Up to three spaces of indent: four is a code block, not a heading.
type Heading = { level: number; text: string };

function readHeading(line: string): Heading | null {
  let i = 0;
  let indent = 0;

  while (i < line.length && isSpaceAt(line, i) && line.charCodeAt(i) !== 0x0a) {
    if (line.charCodeAt(i) !== 0x09) indent++;
    i++;
  }
  if (indent > 3) return null;

  let hashes = 0;
  while (i < line.length && line.charCodeAt(i) === 0x23) {
    hashes++;
    i++;
  }
  if (hashes < 1 || hashes > 6) return null;

  const afterHash = i;
  let spaces = 0;
  while (i < line.length && isSpaceAt(line, i) && line.charCodeAt(i) !== 0x0a) {
    spaces++;
    i++;
  }
  if (spaces === 0 && i < line.length && line.charCodeAt(i) !== 0x0a)
    return null;

  let end = line.length;
  while (end > i && isSpaceAt(line, end - 1)) end--;

  // A closing run of `#` is content, not a closer, unless it stands alone.
  let close = end;
  while (close > i && line.charCodeAt(close - 1) === 0x23) close--;
  if (close < end && close > i && isSpaceAt(line, close - 1)) {
    end = close;
    while (end > i && isSpaceAt(line, end - 1)) end--;
  }
  void afterHash;

  return { level: hashes, text: line.slice(i, end) };
}

function isTagLike(line: string, at: number): boolean {
  return isTagNameStartAt(line, at + 1);
}

function isThematicBreak(line: string): boolean {
  let i = 0;
  let ch = 0;

  while (i < line.length) {
    const c = line.charCodeAt(i);
    if (isSpaceAt(line, i)) {
      i++;
      continue;
    }
    if (c === 0x2d || c === 0x2a || c === 0x5f) {
      if (ch === 0) ch = c;
      if (c !== ch) return false;
      i++;
      continue;
    }
    return false;
  }

  return ch !== 0;
}

function stripQuoteMarker(line: string): string {
  let i = 0;
  while (i < line.length && isSpaceAt(line, i)) i++;
  if (line.charCodeAt(i) !== 0x3e) return line;

  i++;
  if (i < line.length && line.charCodeAt(i) === 0x20) i++;
  return line.slice(i);
}

function normalizeMdLine(line: string): string {
  const { text: masked, restore } = shieldLine(line);

  const list = readList(masked);
  if (list !== null) {
    return restore(list.trim() === "" ? "" : inlineOutside(list));
  }

  const heading = readHeading(masked);
  if (heading !== null) {
    const tag = heading.level <= 2 ? "h2" : "h3";
    return restore(`<${tag}>${inlineOutside(heading.text)}</${tag}>`);
  }

  if (isThematicBreak(masked)) return "";

  return restore(inlineOutside(stripQuoteMarker(masked)));
}

function readList(line: string): string | null {
  if (
    line.indexOf("<ul>") === -1 &&
    line.indexOf("<ol>") === -1 &&
    line.indexOf("<li>") === -1
  ) {
    return null;
  }

  let out = "";
  let i = 0;

  while (i < line.length) {
    const c = line.charCodeAt(i);

    if (c === 0x3c) {
      if (line.startsWith("<ul>", i) || line.startsWith("<ol>", i)) {
        i += 4;
        continue;
      }
      if (line.startsWith("</ul>", i) || line.startsWith("</ol>", i)) {
        i += 5;
        continue;
      }
      if (line.startsWith("<li", i)) {
        let k = i + 3;
        while (k < line.length && line.charCodeAt(k) !== 0x3e) k++;
        out += "- ";
        i = k + 1;
        continue;
      }
      if (line.startsWith("</li>", i)) {
        i += 5;
        continue;
      }
    }

    out += line[i];
    i++;
  }

  return out;
}

/** Cells of a GFM row. A `\|` does not split. */
function tableRow(line: string): string[] | null {
  let from = 0;
  while (from < line.length && isSpaceAt(line, from)) from++;

  let to = line.length;
  while (to > from && isSpaceAt(line, to - 1)) to--;

  if (to - from < 2) return null;
  if (line.charCodeAt(from) !== 0x7c) return null;
  if (line.charCodeAt(to - 1) !== 0x7c) return null;

  const cells: string[] = [];
  let cell = from + 1;
  let i = cell;

  while (i < to - 1) {
    const c = line.charCodeAt(i);
    if (c === 0x5c) {
      i += 2;
      continue;
    }
    if (c === 0x7c) {
      cells.push(unescapePipe(line.slice(cell, i).trim()));
      cell = i + 1;
    }
    i++;
  }
  cells.push(unescapePipe(line.slice(cell, to - 1).trim()));

  return cells;
}

function unescapePipe(s: string): string {
  return s.indexOf("\\|") === -1 ? s : s.replaceAll("\\|", "|");
}

function isDelimiterRow(line: string): boolean {
  let from = 0;
  while (from < line.length && isSpaceAt(line, from)) from++;
  let to = line.length;
  while (to > from && isSpaceAt(line, to - 1)) to--;

  if (from < to && line.charCodeAt(from) === 0x7c) from++;
  if (to > from && line.charCodeAt(to - 1) === 0x7c) to--;

  let dashes = 0;
  for (let i = from; i < to; i++) {
    const c = line.charCodeAt(i);
    if (c === 0x2d) {
      dashes++;
      continue;
    }
    if (c === 0x3a || c === 0x7c || isSpaceAt(line, i)) continue;
    return false;
  }

  return dashes > 0;
}

/** No delimiter row yet, so the header line stays a paragraph. */
function tryTable(
  lines: string[],
  i: number,
  final: boolean,
): { xml: string; next: number } | null {
  if (i + 1 >= lines.length && !final) return null;

  const head = tableRow(lines[i]);
  if (head === null) return null;
  if (i + 1 >= lines.length) return null;
  if (!isDelimiterRow(lines[i + 1])) return null;

  let xml = "<table><tr>";
  for (const c of head) xml += `<th>${inlineOutside(c)}</th>`;
  xml += "</tr>";

  let j = i + 2;
  while (j < lines.length) {
    const cells = tableRow(lines[j]);
    if (cells === null || cells.length !== head.length) break;
    xml += "<tr>";
    for (const c of cells) xml += `<td>${inlineOutside(c)}</td>`;
    xml += "</tr>";
    j++;
  }

  return { xml: xml + "</table>", next: j };
}

export function normalizeMd(
  src: string,
  opts: { final?: boolean } = {},
): string {
  const final = opts.final !== false;
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
  let openTags: string[] = [];

  while (k < lines.length) {
    if (depth > 0) {
      const line = lines[k];
      const inner = openTags[openTags.length - 1];

      out.push(
        inner !== undefined && PROSE_TAGS.has(inner)
          ? normalizeMdLine(line)
          : line,
      );

      depth = Math.max(0, depth + depthDelta(line));
      openTags = depth === 0 ? [] : trackTags(openTags, line);
      k++;
      continue;
    }

    const fence = readFence(lines[k]);
    if (fence !== null) {
      if (!inFence) {
        inFence = true;
        fenceLang = fence;
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

    // A `<` with no `>` yet passes raw. Holding prose there blanks the rest of
    // the message, and the buffer is re-read next delta anyway.
    const line = lines[k];
    const lt = line.indexOf("<");

    if (
      lt !== -1 &&
      scanTagEnd(line, lt + 1).kind === "unterminated" &&
      isTagLike(line, lt)
    ) {
      out.push(line);
      k++;
      continue;
    }

    const d = depthDelta(line);

    if (d > 0) {
      out.push(line);
    } else {
      const t = tryTable(lines, k, final);
      if (t !== null) {
        out.push(t.xml);
        k = t.next;
        continue;
      }
      out.push(normalizeMdLine(line));
    }
    depth = Math.max(0, depth + d);
    openTags = depth === 0 ? [] : trackTags(openTags, line);
    k++;
  }

  if (inFence) flushFence(false);

  return out.join("\n");
}
