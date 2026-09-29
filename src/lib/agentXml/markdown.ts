// Rendering. The line-level pass: code fences and tag bodies are shielded so
// Markdown can never rewrite a tool's JSON, then tables, headings and lists are
// rebuilt line by line. Inline formatting is in `inline.ts`.
import { inlineOutside } from "./inline";
import { scanTagEnd } from "./tokenize";
import {
  Char,
  classifyAt,
  isSpaceAt,
  isTagNameCharAt,
  isTagNameStartAt,
} from "./scan";

// Component tags whose bodies can span lines. Tables and headings are
// prose-level and never counted; inline tags never span lines.
/**
 * Tags whose body can span lines, and the ones among them the model writes as
 * prose. A plan step is something the user reads, so its markdown is normalized
 * like any other prose; a terminal or a diff stays raw on purpose, because a
 * shell script's `#` is a comment and not a heading.
 */
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

/** One tag found in a line. */
type Span = { name: string; close: boolean; self: boolean; end: number };

/**
 * Every spanning tag in `line`, in order.
 *
 * A single pass with the same quote-aware scan the tokenizer uses, so a `>`
 * inside a quoted attribute — `command="ls > f"` — cannot end the tag early
 * and leave the rest of the line to be read as markup. The previous version
 * spelled this as one alternation; it is a scanner because it has to agree
 * with the tokenizer about where a tag ends, and two different answers to that
 * question is how a line ends up half-raw and half-formatted.
 */
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

/**
 * Replace every complete single-line payload block with an opaque placeholder,
 * so prose normalization never rewrites its body — a backtick in a command
 * would otherwise break the arg parsing on the way back out, and a `#` in a
 * diff would turn into a heading.
 *
 * Prose bodies are deliberately left alone: those are the text the user is
 * meant to read, and it has to be normalized.
 *
 * A block is only shielded when its own closing tag is on the same line. A
 * payload that runs to the next line is handled by the depth tracker instead,
 * which has already put the following lines inside the body.
 */
/** The language on a code-fence opener, or `null` if the line is not one. */
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

  // Nothing but spaces may follow, or this is prose that happens to start
  // with three backticks.
  while (k < line.length) {
    if (isSpaceAt(line, k) && line.charCodeAt(k) !== 0x0a) {
      k++;
      continue;
    }
    return null;
  }

  return lang;
}

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

/** The tag name in `line[from..to]`, with its close/self flags, or `null`. */
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

/**
 * Escape the three characters that would otherwise read back as markup. A scan
 * rather than three chained replacements: a code fence is usually text with no
 * markup in it at all, and this returns it untouched without rebuilding it.
 */
/** Put the shielded spans back, in one pass, without a regular expression. */
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

/**
 * An ATX heading, per CommonMark 4.2: up to three spaces of indentation, one to
 * six `#`, then a space or the end of the line, with an optional closing run of
 * `#`.
 *
 * The indentation allowance is the fix for `# Heading` arriving indented — the
 * previous pattern anchored at column zero, so a heading a model indented by
 * one space or a tab fell through as literal text. Four spaces is a code block,
 * not a heading, which is why the allowance stops at three.
 */
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

  // The `#` must be followed by a space or tab, or be the whole line.
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

/**
 * Whether the `<` at `at` begins something that could still become a tag. A
 * `<` followed by a letter is one; `<1000` and `a < b` are not.
 */
function isTagLike(line: string, at: number): boolean {
  return isTagNameStartAt(line, at + 1);
}

/** A thematic break: three or more of one of `-`, `*`, `_`, spaces allowed. */
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

/** Strip a leading `>` blockquote marker, and the space that usually follows. */
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

/**
 * `<ul>/<ol>/<li>` rewritten as `- ` bullets.
 *
 * A model that writes a list as markup should still get a list. Returns the
 * line with the tags replaced, or `null` when the line has no list markup.
 */
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

/**
 * Split a table row into its cells, or `null` when the line is not a row.
 *
 * A row is a line that starts and ends with a pipe. Cells are split on pipes
 * and trimmed, which is what a GFM table means. A `\|` is an escaped pipe and
 * belongs to the cell, so it does not split.
 */
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

/**
 * The delimiter row under a header: pipes, colons, dashes and spaces only, and
 * at least one dash. Alignment is read from the colons, which is what they are
 * for.
 */
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
    // Colons carry the alignment, pipes separate the cells, spaces are
    // allowed between everything. Anything else means this is not a
    // delimiter row at all.
    if (c === 0x3a || c === 0x7c || isSpaceAt(line, i)) continue;
    return false;
  }

  return dashes > 0;
}

/**
 * Rebuild a table starting at line `i`, returning the XML and the index of the
 * first line that is not part of it.
 *
 * A table is only built once its delimiter row has arrived. While the table is
 * still streaming, the row that will become the header is a paragraph until
 * then — rendering a table that later turns out not to be one is worse than
 * showing the text and correcting it on the next line.
 */
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

    // A line whose `<` has not found its `>` yet is passed through raw, for
    // that line only.
    //
    // It is tempting to hold the line back on a live turn until the tag
    // closes. Do not: a `<` with a letter after it is extremely common in
    // ordinary prose ("use <b> for bold", "the route is a -> b"), and holding
    // cost the rest of the message every time one appeared — a model writing
    // `a < b` early in an answer left everything after it unrendered until the
    // turn ended, which is what made the chat look like it had stopped parsing
    // and then fixed itself on reload.
    //
    // Passing the line through is safe because the whole buffer is re-read on
    // the next delta. The line is examined again with more of the message
    // behind it, formats correctly if the tag turns out to have closed, and
    // costs at worst a brief flash of raw text on the one line mid-tag.
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
