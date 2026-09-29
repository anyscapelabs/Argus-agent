// Rendering. The line-level pass: code fences and tag bodies are shielded so
// Markdown can never rewrite a tool's JSON, then tables, headings and lists are
// rebuilt line by line. Inline formatting is in `inline.ts`.
import { inlineOutside } from "./inline";
import { scanTagEnd } from "./tokenize";
import { isSpaceAt, isTagNameStartAt } from "./scan";

// Component tags whose bodies can span lines. Tables and headings are
// prose-level and never counted; inline tags never span lines.
const PROSE_BODY = "thinking|plan|step|warning|error";
const PAYLOAD_BODY =
  "action|approval|diff|terminal|sandbox|email-draft|browser-action|memory-ref|codeblock";
const DEPTH_TAGS = `${PROSE_BODY}|${PAYLOAD_BODY}`;
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

// Which of these bodies the model writes as prose, as opposed to payload. A
// plan step or a thought is something the user reads, so its markdown has to
// be normalized like any other prose or it reaches them as the characters the
// model typed. A terminal or a diff stays raw on purpose: a shell script's
// `#` is a comment, not a heading.
const PROSE_TAGS = new Set(PROSE_BODY.split("|"));

function trackTags(stack: string[], line: string): string[] {
  const next = [...stack];
  let m: RegExpExecArray | null;
  DEPTH_RE.lastIndex = 0;
  while ((m = DEPTH_RE.exec(line)) !== null) {
    const t = m[0];
    if (t.startsWith("</")) {
      next.pop();
    } else if (!t.endsWith("/>")) {
      next.push(t.slice(1).split(/[\s/>]/, 1)[0]);
    }
  }
  return next;
}

// Complete single-line payload blocks (`<action>{...}</action>`) are shielded
// so prose normalization never rewrites their bodies (backticks in commands
// would otherwise break arg parsing and hide the step hint). Prose bodies are
// deliberately not shielded: they are the text the user is meant to read.
const SINGLE_RE = new RegExp(
  `<(${PAYLOAD_BODY})\\b${TAG_ATTRS}>.*?</\\1>`,
  "g",
);

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
    restorePlaceholders(s, saved);
  return { text, restore };
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
  if (spaces === 0 && i < line.length && line.charCodeAt(i) !== 0x0a) return null;

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
  if (line.indexOf("<ul>") === -1 && line.indexOf("<ol>") === -1 && line.indexOf("<li>") === -1) {
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

export function normalizeMd(src: string, opts: { final?: boolean } = {}): string {
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

    // A line with no tag end is raw for that line only. Latching a flag
    // here suppressed markdown for the rest of the message after one stray `<`.
    //
    // On a live turn it is not raw at all: the `<` may be the start of a tag
    // that has not finished arriving, so the line is held and re-examined when
    // the next chunk lands. Treating it as prose on the way past is what left
    // a heading showing as literal `#` text whenever a command somewhere above
    // it contained a bare `<`.
    const line = lines[k];
    const lt = line.indexOf("<");

    if (lt !== -1 && scanTagEnd(line, lt + 1).kind === "unterminated" && isTagLike(line, lt)) {
      if (!final) break;
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
