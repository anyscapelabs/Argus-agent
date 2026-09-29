// Rendering. Inline formatting first, then the line-level pass: code fences
// and tag bodies are shielded so Markdown can never rewrite a tool's JSON, and
// tables are rebuilt row by row.
import { findTagEnd } from "./tokenize";
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

export function normalizeMd(src: string): string {
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
    const line = lines[k];
    const lt = line.indexOf("<");

    if (lt !== -1 && findTagEnd(line, lt + 1) === -1) {
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
    openTags = depth === 0 ? [] : trackTags(openTags, line);
    k++;
  }

  if (inFence) flushFence(false);

  return out.join("\n");
}
