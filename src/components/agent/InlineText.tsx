import { Fragment } from "react";
import {
  SiGmail,
  SiGooglecalendar,
  SiGooglechrome,
  SiGoogledocs,
  SiGoogledrive,
  SiGooglemeet,
  SiGooglesheets,
  SiGoogleslides,
  SiGithub,
} from "react-icons/si";

import type { InlineNode } from "../../lib/agentXml";
import { ExtLink } from "../../lib/extLink";
import PathChip from "./PathChip";

const INLINE_CLS: Record<string, string> = {
  bold: "font-bold",
  italic: "italic",
  underline: "underline",
  strikethrough: "line-through",
  code: "rounded bg-bg-secondary px-1 py-0.5 align-middle font-mono text-[0.9em]",
};

export function renderInline(nodes: InlineNode[]): React.ReactNode {
  const out: React.ReactNode[] = [];
  let k = 0;

  type StackItem = { tag: string; href?: string };
  const stk: StackItem[] = [];

  const flush = (txt: string) => {
    if (!txt) {
      return;
    }

    const urlRe = /(https?:\/\/[^\s]+)/g;
    const mentionRe = /(@[a-zA-Z0-9_]+)/g;
    const toolSet = new Set([
      "gmail",
      "chrome",
      "drive",
      "docs",
      "sheets",
      "slides",
      "meet",
      "calendar",
      "github",
    ]);
    const urlParts = txt.split(urlRe);

    for (const up of urlParts) {
      if (!up) {
        continue;
      }

      if (urlRe.test(up)) {
        urlRe.lastIndex = 0;
        out.push(
          <ExtLink key={`i-${k++}`} href={up}>
            {up}
          </ExtLink>,
        );
        continue;
      }

      const emailRe = /([a-zA-Z0-9._%+-]+@[a-zA-Z0-9-]+(?:\.[a-zA-Z0-9-]+)+)/g;
      const eParts = up.split(emailRe);

      for (let ei = 0; ei < eParts.length; ei++) {
        const ep = eParts[ei];
        if (!ep) {
          continue;
        }

        if (ei % 2 === 1) {
          pushPlain(ep);
          continue;
        }

        const mParts = ep.split(mentionRe);

        for (const mp of mParts) {
          if (!mp) {
            continue;
          }

          if (mentionRe.test(mp)) {
            mentionRe.lastIndex = 0;
            const tool = mp.slice(1).toLowerCase();
            if (toolSet.has(tool)) {
              const toolMeta: Record<
                string,
                {
                  Icon: React.ComponentType<{
                    size?: number;
                    className?: string;
                  }>;
                  color: string;
                }
              > = {
                chrome: { Icon: SiGooglechrome, color: "text-[#4285F4]" },
                gmail: { Icon: SiGmail, color: "text-[#EA4335]" },
                drive: { Icon: SiGoogledrive, color: "text-[#4285F4]" },
                docs: { Icon: SiGoogledocs, color: "text-[#4285F4]" },
                sheets: { Icon: SiGooglesheets, color: "text-[#0F9D58]" },
                slides: { Icon: SiGoogleslides, color: "text-[#F4B400]" },
                meet: { Icon: SiGooglemeet, color: "text-[#00897B]" },
                calendar: {
                  Icon: SiGooglecalendar,
                  color: "text-[#4285F4]",
                },
                github: { Icon: SiGithub, color: "text-text-primary" },
              };
              const meta = toolMeta[tool];
              const { Icon, color } = meta;
              out.push(
                <span
                  key={`i-${k++}`}
                  className="inline-flex items-center gap-1 rounded border border-border-primary bg-bg-secondary px-1.5 py-0.5 font-sans text-xs font-medium text-text-secondary"
                >
                  <Icon size={12} className={color} />
                  {mp}
                </span>,
              );
              continue;
            }

            out.push(
              <span
                key={`i-${k++}`}
                className="rounded bg-blue-500/15 px-1 py-0.5 font-medium text-blue-300"
              >
                {mp}
              </span>,
            );
            continue;
          }

          pushText(mp);
        }
      }
    }
  };

  const pushPlain = (seg: string) => {
    if (!seg) {
      return;
    }

    const cls = stk
      .filter((s) => s.tag !== "link")
      .map((s) => INLINE_CLS[s.tag])
      .filter(Boolean)
      .join(" ");
    const link = stk.find((s) => s.tag === "link");

    if (link?.href) {
      out.push(
        cls ? (
          <span key={`i-${k++}`} className={cls}>
            <ExtLink href={link.href}>{seg}</ExtLink>
          </span>
        ) : (
          <ExtLink key={`i-${k++}`} href={link.href}>
            {seg}
          </ExtLink>
        ),
      );
      return;
    }

    if (!cls) {
      out.push(<Fragment key={`i-${k++}`}>{seg}</Fragment>);
      return;
    }

    out.push(
      <span key={`i-${k++}`} className={cls}>
        {seg}
      </span>,
    );
  };

  const pushText = (seg: string) => {
    if (!seg) {
      return;
    }

    const re = /(?:~\/)?[^\s<>"'`]*\/[^\s<>"'`]+/g;
    let cur = 0;
    let m: RegExpExecArray | null;
    const plain = (from: number, to: number) => {
      if (to <= from) {
        return;
      }
      pushPlain(seg.slice(from, to));
    };

    while ((m = re.exec(seg)) !== null) {
      const raw = m[0];
      const junk = raw.match(/[.,;:!?)\]}]+$/)?.[0].length ?? 0;
      const path = raw.slice(0, raw.length - junk).replace(/\/+$/, "");

      const slashes = (path.match(/\//g) ?? []).length;
      const base = path.split("/").pop() ?? "";
      const usable =
        path.length >= 2 &&
        !path.startsWith("//") &&
        (path.startsWith("~/") ||
          path.startsWith("/") ||
          slashes >= 2 ||
          base.includes("."));

      if (!usable) {
        continue;
      }

      plain(cur, m.index);
      out.push(<PathChip key={`i-${k++}`} path={path} />);
      cur = m.index + path.length;
      re.lastIndex = cur;
    }

    plain(cur, seg.length);
  };

  const scan = (txt: string) => {
    const re = /<\/?([a-z][a-z0-9-]*)(?:\s+href="([^"]*)")?\s*\/?>/g;
    let cur = 0;
    let m: RegExpExecArray | null;
    let pend = "";

    while ((m = re.exec(txt)) !== null) {
      const raw = m[0];
      const name = m[1].toLowerCase();
      const href = m[2];
      const isClose = raw.startsWith("</");
      const isSelf = raw.endsWith("/>");
      const isInline = name in INLINE_CLS || name === "link";

      if (!isInline) {
        continue;
      }

      flush(pend + txt.slice(cur, m.index));
      pend = "";

      if (isClose) {
        const idx = stk.findLastIndex((s: StackItem) => s.tag === name);
        if (idx === -1) {
          cur = m.index + raw.length;
          continue;
        }

        stk.splice(idx, 1);
        cur = m.index + raw.length;
        continue;
      }

      if (isSelf) {
        cur = m.index + raw.length;
        continue;
      }

      if (name === "link") {
        stk.push({ tag: name, href });
      } else {
        stk.push({ tag: name });
      }

      cur = m.index + raw.length;
    }

    flush(pend + txt.slice(cur));
  };

  for (const n of nodes) {
    scan(n.value);
  }

  return out;
}
