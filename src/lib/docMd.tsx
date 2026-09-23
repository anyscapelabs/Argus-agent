import type { ReactNode } from "react";

import { ExtLink } from "./extLink";

const INLINE_RE = /(\*\*[^*\n]+\*\*|\*[^*\n]+\*|`[^`]+`|\[[^\]]+\]\([^)]+\))/g;

function inline(text: string, key: string): ReactNode[] {
  const parts = text.split(INLINE_RE).filter((p) => p !== "");
  const out: ReactNode[] = [];

  for (let i = 0; i < parts.length; i++) {
    const p = parts[i];
    const k = `${key}-${i}`;

    if (p.startsWith("`") && p.endsWith("`") && p.length > 2) {
      out.push(
        <code
          key={k}
          className="rounded bg-bg-hover-secondary px-1 py-0.5 font-mono text-[0.85em] text-text-primary"
        >
          {p.slice(1, -1)}
        </code>,
      );
    } else if (p.startsWith("**") && p.endsWith("**")) {
      out.push(
        <strong key={k} className="font-semibold text-text-primary">
          {p.slice(2, -2)}
        </strong>,
      );
    } else if (p.startsWith("*") && p.endsWith("*")) {
      out.push(
        <em key={k} className="text-text-primary">
          {p.slice(1, -1)}
        </em>,
      );
    } else if (p.startsWith("[")) {
      const m = /\[([^\]]+)\]\(([^)]+)\)/.exec(p);

      if (m) {
        out.push(
          <ExtLink key={k} href={m[2]}>
            {m[1]}
          </ExtLink>,
        );
      } else {
        out.push(p);
      }
    } else {
      out.push(p);
    }
  }

  return out;
}

const BULLET_RE = /^\s*[-*]\s+/;
const NUM_RE = /^\s*\d+\.\s+/;
const TABLE_SEP_RE = /^\|?\s*:?-{1,}:?\s*(\|\s*:?-{1,}:?\s*)*\|?\s*$/;

function splitRow(line: string): string[] {
  let t = line.trim();
  if (t.startsWith("|")) t = t.slice(1);
  if (t.endsWith("|")) t = t.slice(0, -1);
  return t.split("|").map((c) => c.trim());
}

function alignsOf(sep: string[]): string[] {
  return sep.map((c) => {
    const left = c.startsWith(":");
    const right = c.endsWith(":");
    if (left && right) return "center";
    if (right) return "right";
    return "left";
  });
}

function renderTable(
  head: string[],
  aligns: string[],
  body: string[][],
  key: number,
): ReactNode {
  const alignCls = (a: string) =>
    a === "center" ? "text-center" : a === "right" ? "text-right" : "text-left";

  return (
    <div
      key={key}
      className="my-3 overflow-x-auto rounded-lg border border-border-primary"
    >
      <table className="w-full border-collapse text-[15px] leading-6">
        <thead>
          <tr className="border-b border-border-primary">
            {head.map((c, j) => (
              <th
                key={j}
                className={`px-3 py-2 font-semibold text-text-primary ${alignCls(aligns[j] ?? "left")}`}
              >
                {inline(c, `th${key}-${j}`)}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {body.map((r, i) => (
            <tr
              key={i}
              className={i > 0 ? "border-t border-border-primary/50" : undefined}
            >
              {r.map((c, j) => (
                <td
                  key={j}
                  className={`px-3 py-2 align-top text-text-primary ${alignCls(aligns[j] ?? "left")}`}
                >
                  {inline(c, `td${key}-${i}-${j}`)}
                </td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

export function renderDocMd(md: string): ReactNode {
  const lines = md.split("\n");
  const out: ReactNode[] = [];
  let i = 0;
  let key = 0;

  while (i < lines.length) {
    const line = lines[i];

    if (line.startsWith("```")) {
      const lang = line.slice(3).trim();
      const body: string[] = [];
      i++;

      while (i < lines.length && !lines[i].startsWith("```")) {
        body.push(lines[i]);
        i++;
      }
      i++;

      out.push(
        <div key={key++} className="my-3 overflow-hidden rounded-lg border border-border-primary">
          {lang !== "" && (
            <div className="border-b border-border-primary px-3 py-1 font-mono text-[10px] text-text-secondary">
              {lang}
            </div>
          )}
          <pre className="overflow-x-auto px-3 py-2 font-mono text-xs leading-5 text-text-primary">
            {body.join("\n")}
          </pre>
        </div>,
      );
      continue;
    }

    if (
      line.includes("|") &&
      i + 1 < lines.length &&
      TABLE_SEP_RE.test(lines[i + 1].trim())
    ) {
      const head = splitRow(line);
      const aligns = alignsOf(splitRow(lines[i + 1]));
      const body: string[][] = [];
      i += 2;

      while (i < lines.length && lines[i].includes("|") && lines[i].trim() !== "") {
        body.push(splitRow(lines[i]));
        i++;
      }

      out.push(renderTable(head, aligns, body, key++));
      continue;
    }

    if (line.startsWith("#### ")) {
      out.push(
        <h4 key={key++} className="mt-4 mb-1 text-[16px] font-semibold text-text-primary">
          {inline(line.slice(5), `h${key}`)}
        </h4>,
      );
      i++;
      continue;
    }

    if (line.startsWith("### ")) {
      out.push(
        <h3 key={key++} className="mt-5 mb-2 text-[17px] font-semibold text-text-primary">
          {inline(line.slice(4), `h${key}`)}
        </h3>,
      );
      i++;
      continue;
    }

    if (line.startsWith("## ")) {
      out.push(
        <h2 key={key++} className="mt-5 mb-2 text-[19px] font-semibold text-text-primary">
          {inline(line.slice(3), `h${key}`)}
        </h2>,
      );
      i++;
      continue;
    }

    if (line.startsWith("# ")) {
      out.push(
        <h1 key={key++} className="mt-5 mb-2 text-[22px] font-semibold text-text-primary">
          {inline(line.slice(2), `h${key}`)}
        </h1>,
      );
      i++;
      continue;
    }

    if (line.startsWith("> ")) {
      const body: string[] = [];

      while (i < lines.length && lines[i].startsWith("> ")) {
        body.push(lines[i].slice(2));
        i++;
      }

      out.push(
        <blockquote
          key={key++}
          className="my-3 border-l-2 border-border-primary pl-3 leading-7 text-text-secondary"
        >
          {inline(body.join(" "), `q${key}`)}
        </blockquote>,
      );
      continue;
    }

    if (line.trim() === "---") {
      out.push(<hr key={key++} className="my-4 border-border-primary" />);
      i++;
      continue;
    }

    if (BULLET_RE.test(line) || NUM_RE.test(line)) {
      const ordered = NUM_RE.test(lines[i]);
      const items: string[] = [];

      while (i < lines.length && (ordered ? NUM_RE : BULLET_RE).test(lines[i])) {
        items.push(lines[i].replace(ordered ? NUM_RE : BULLET_RE, ""));
        i++;
      }

      const rendered = items.map((it, n) => (
        <li key={n} className="leading-7">
          {inline(it, `li${key}-${n}`)}
        </li>
      ));

      out.push(
        ordered ? (
          <ol key={key++} className="my-2 list-decimal space-y-1 pl-5 text-text-primary">
            {rendered}
          </ol>
        ) : (
          <ul key={key++} className="my-2 list-disc space-y-1 pl-5 text-text-primary">
            {rendered}
          </ul>
        ),
      );
      continue;
    }

    if (line.trim() === "") {
      i++;
      continue;
    }

    const para: string[] = [];

    while (
      i < lines.length &&
      lines[i].trim() !== "" &&
      !/^(#{1,4} |```|> |---)/.test(lines[i]) &&
      !BULLET_RE.test(lines[i]) &&
      !NUM_RE.test(lines[i]) &&
      !(lines[i].includes("|") && i + 1 < lines.length && TABLE_SEP_RE.test(lines[i + 1].trim()))
    ) {
      para.push(lines[i]);
      i++;
    }

    out.push(
      <p key={key++} className="my-2 leading-7 text-text-primary">
        {inline(para.join(" "), `p${key}`)}
      </p>,
    );
  }

  return <div>{out}</div>;
}
