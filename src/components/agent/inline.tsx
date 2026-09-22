import type { ReactNode } from "react";

const CLS: Record<string, string> = {
  bold: "font-bold",
  italic: "italic",
  underline: "underline",
  strikethrough: "line-through",
  code: "rounded bg-bg-secondary px-1 py-0.5 align-middle font-mono text-[0.9em]",
};

const TAG_RE =
  /<(bold|italic|underline|strikethrough|code|link)(?:\s+href="([^"]*)")?>([\s\S]*?)<\/\1>|<\/(bold|italic|underline|strikethrough|code|link)>/g;

export function renderInlineText(text: string): ReactNode {
  const out: ReactNode[] = [];
  let k = 0;
  let last = 0;
  let m: RegExpExecArray | null;
  const re = new RegExp(TAG_RE);

  while ((m = re.exec(text)) !== null) {
    if (m.index > last) out.push(text.slice(last, m.index));

    if (m[1] !== undefined) {
      const inner = m[3];

      if (m[1] === "link") {
        out.push(
          <a
            key={k++}
            href={m[2] ?? ""}
            target="_blank"
            rel="noreferrer"
            className="text-blue-400 underline decoration-blue-400/30 underline-offset-2 hover:text-blue-300"
          >
            {inner}
          </a>,
        );
      } else {
        out.push(
          <span key={k++} className={CLS[m[1]]}>
            {inner}
          </span>,
        );
      }
    } else {
      out.push(m[0]);
    }

    last = m.index + m[0].length;
  }

  if (last < text.length) out.push(text.slice(last));

  return out;
}
