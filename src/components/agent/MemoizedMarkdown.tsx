import { marked } from "marked";
import { memo, useMemo, useState } from "react";
import ReactMarkdown from "react-markdown";
import rehypeRaw from "rehype-raw";
import remarkGfm from "remark-gfm";

import { ExtLink } from "../../lib/extLink";
import PathChip from "./PathChip";

function parseMarkdownIntoBlocks(markdown: string): string[] {
  const tokens = marked.lexer(markdown);
  return tokens.map((token) => token.raw);
}

// Our pipeline emits <bold>/<italic>/<underline>/<strikethrough>/<link>.
// Rehype treats <link> as a head void element and drops its children, so
// translate to body-safe HTML before ReactMarkdown sees it.
function prepXmlTags(src: string): string {
  return src
    .replaceAll("<bold>", "<strong>")
    .replaceAll("</bold>", "</strong>")
    .replaceAll("<italic>", "<em>")
    .replaceAll("</italic>", "</em>")
    .replaceAll("<underline>", "<u>")
    .replaceAll("</underline>", "</u>")
    .replaceAll("<strikethrough>", "<del>")
    .replaceAll("</strikethrough>", "</del>")
    .replaceAll("</link>", "</a>")
    .replaceAll("<link>", "<a>")
    .replace(/<link\s+href="/g, '<a href="');
}

function isHttp(href: string | undefined): boolean {
  if (!href) return false;
  const lower = href.slice(0, 8).toLowerCase();
  return lower.startsWith("http://") || lower.startsWith("https://");
}

const TOOL_SET = new Set([
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

let chipKey = 0;
const nextKey = (p: string) => `${p}-${chipKey++}`;

function chipify(text: string): React.ReactNode[] {
  const out: React.ReactNode[] = [];
  const urlRe = /(https?:\/\/[^\s]+)/g;
  const emailRe = /([a-zA-Z0-9._%+-]+@[a-zA-Z0-9-]+(?:\.[a-zA-Z0-9-]+)+)/g;
  const mentionRe = /(@[a-zA-Z0-9_]+)/g;

  for (const up of text.split(urlRe)) {
    if (!up) continue;

    if (urlRe.test(up)) {
      urlRe.lastIndex = 0;
      out.push(
        <ExtLink key={nextKey("u")} href={up}>
          {up}
        </ExtLink>,
      );
      continue;
    }

    for (const [ei, ep] of up.split(emailRe).entries()) {
      if (!ep) continue;

      // Shielded: an address never becomes chips or links.
      if (ei % 2 === 1) {
        out.push(<span key={nextKey("e")}>{ep}</span>);
        continue;
      }

      for (const mp of ep.split(mentionRe)) {
        if (!mp) continue;

        if (mentionRe.test(mp)) {
          mentionRe.lastIndex = 0;
          const tool = mp.slice(1).toLowerCase();

          if (TOOL_SET.has(tool)) {
            out.push(
              <span
                key={nextKey("m")}
                className="inline-flex items-center gap-1 rounded border border-border-primary bg-bg-secondary px-1.5 py-0.5 font-sans text-xs font-medium text-text-secondary"
              >
                {mp}
              </span>,
            );
          } else {
            out.push(
              <span
                key={nextKey("m")}
                className="rounded bg-blue-500/15 px-1 py-0.5 font-medium text-blue-300"
              >
                {mp}
              </span>,
            );
          }
          continue;
        }

        out.push(...pathify(mp));
      }
    }
  }

  return out;
}

function pathify(seg: string): React.ReactNode[] {
  const out: React.ReactNode[] = [];
  const re = /(?:~\/)?[^\s<>"'`]*\/[^\s<>"'`]+/g;
  let cur = 0;
  let m: RegExpExecArray | null;

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

    if (!usable) continue;

    if (m.index > cur) out.push(<span key={nextKey("t")}>{seg.slice(cur, m.index)}</span>);
    out.push(<PathChip key={nextKey("p")} path={path} />);
    cur = m.index + path.length;
    re.lastIndex = cur;
  }

  if (cur < seg.length) out.push(<span key={nextKey("t")}>{seg.slice(cur)}</span>);

  return out;
}

function SafeImage({ src, alt }: { src?: string; alt?: string }) {
  const [dead, setDead] = useState(false);

  if (!src || !isHttp(src) || dead) {
    return alt ? <span>{alt}</span> : null;
  }

  return (
    <ExtLink href={src}>
      <img
        src={src}
        alt={alt ?? ""}
        loading="lazy"
        onError={() => setDead(true)}
        className="h-auto w-full max-w-[480px] rounded-lg border border-border-primary object-contain"
      />
    </ExtLink>
  );
}

function mdChildren(children: React.ReactNode): React.ReactNode {
  if (typeof children === "string") return <>{chipify(children)}</>;
  if (Array.isArray(children)) {
    return (
      <>
        {children.map((c, i) =>
          typeof c === "string" ? <span key={i}>{chipify(c)}</span> : c,
        )}
      </>
    );
  }
  return children;
}

const MD_COMPONENTS = {
  a: ({ href, children }: { href?: string; children?: React.ReactNode }) => {
    if (!href) return <>{children}</>;

    // Never a live link for mailto/relative/javascript — text only.
    if (href.startsWith("mailto:") || !isHttp(href)) {
      return <span>{children}</span>;
    }

    return <ExtLink href={href}>{children}</ExtLink>;
  },
  img: ({ src, alt }: { src?: string; alt?: string }) => (
    <SafeImage src={src} alt={alt} />
  ),
  strong: ({ children }: { children?: React.ReactNode }) => (
    <span className="font-bold">{mdChildren(children)}</span>
  ),
  em: ({ children }: { children?: React.ReactNode }) => (
    <span className="italic">{mdChildren(children)}</span>
  ),
  u: ({ children }: { children?: React.ReactNode }) => (
    <span className="underline">{mdChildren(children)}</span>
  ),
  del: ({ children }: { children?: React.ReactNode }) => (
    <span className="line-through">{mdChildren(children)}</span>
  ),
  p: ({ children }: { children?: React.ReactNode }) => (
    <span className="font-sans text-[16px] font-medium leading-6 text-text-primary">
      {mdChildren(children)}
    </span>
  ),
  ul: ({ children }: { children?: React.ReactNode }) => (
    <span className="mt-2 ml-4 flex list-disc flex-col gap-1">{children}</span>
  ),
  ol: ({ children }: { children?: React.ReactNode }) => (
    <span className="mt-2 ml-4 flex list-decimal flex-col gap-1">{children}</span>
  ),
  li: ({ children }: { children?: React.ReactNode }) => (
    <span className="font-sans text-[16px] font-medium leading-6 text-text-primary">
      {mdChildren(children)}
    </span>
  ),
  h1: ({ children }: { children?: React.ReactNode }) => (
    <span className="text-[20px] font-bold">{mdChildren(children)}</span>
  ),
  h2: ({ children }: { children?: React.ReactNode }) => (
    <span className="text-[18px] font-semibold">{mdChildren(children)}</span>
  ),
  h3: ({ children }: { children?: React.ReactNode }) => (
    <span className="text-[16px] font-semibold">{mdChildren(children)}</span>
  ),
  h4: ({ children }: { children?: React.ReactNode }) => (
    <span className="text-sm font-semibold text-text-secondary">
      {mdChildren(children)}
    </span>
  ),
  blockquote: ({ children }: { children?: React.ReactNode }) => (
    <span className="border-l-2 border-border-primary pl-3 font-serif text-[16px] font-light leading-6 text-text-secondary">
      {children}
    </span>
  ),
  hr: () => <span className="my-3 block border-t border-border-primary" />,
  table: ({ children }: { children?: React.ReactNode }) => (
    <span className="block overflow-x-auto rounded-lg border border-border-primary">
      <table className="w-full border-collapse text-sm">{children}</table>
    </span>
  ),
  thead: ({ children }: { children?: React.ReactNode }) => <thead>{children}</thead>,
  tbody: ({ children }: { children?: React.ReactNode }) => <tbody>{children}</tbody>,
  tr: ({ children }: { children?: React.ReactNode }) => (
    <tr className="border-t border-border-primary first:border-t-0">{children}</tr>
  ),
  th: ({ children }: { children?: React.ReactNode }) => (
    <th className="px-3 py-2 text-left font-medium text-text-primary">
      {mdChildren(children)}
    </th>
  ),
  td: ({ children }: { children?: React.ReactNode }) => (
    <td className="px-3 py-2 text-left font-light text-text-secondary">
      {mdChildren(children)}
    </td>
  ),
  pre: ({ children }: { children?: React.ReactNode }) => (
    <div className="mt-2 overflow-hidden rounded-lg border border-border-primary first:mt-0">
      <pre className="overflow-x-auto px-3 py-2 font-mono text-xs leading-5 text-text-primary">
        {children}
      </pre>
    </div>
  ),
  code: ({
    className,
    children,
  }: {
    className?: string;
    children?: React.ReactNode;
  }) => {
    const lang = /language-(\w+)/.exec(className ?? "")?.[1];

    if (lang) {
      return (
        <>
          <div className="border-b border-border-primary px-3 py-1 font-mono text-[10px] text-text-secondary">
            {lang}
          </div>
          <code className="font-mono text-xs leading-5">{children}</code>
        </>
      );
    }

    return (
      <code className="rounded bg-bg-secondary px-1 py-0.5 align-middle font-mono text-[0.9em]">
        {children}
      </code>
    );
  },
};

const MemoizedMarkdownBlock = memo(
  ({ content }: { content: string }) => {
    const prepared = useMemo(() => prepXmlTags(content), [content]);

    return (
      <ReactMarkdown
        remarkPlugins={[remarkGfm]}
        rehypePlugins={[rehypeRaw]}
        components={MD_COMPONENTS}
      >
        {prepared}
      </ReactMarkdown>
    );
  },
  (prevProps, nextProps) => {
    if (prevProps.content !== nextProps.content) return false;
    return true;
  },
);

MemoizedMarkdownBlock.displayName = "MemoizedMarkdownBlock";

export const MemoizedMarkdown = memo(
  ({ content, id }: { content: string; id: string }) => {
    const blocks = useMemo(() => parseMarkdownIntoBlocks(content), [content]);

    return blocks.map((block, index) => (
      <MemoizedMarkdownBlock content={block} key={`${id}-block_${index}`} />
    ));
  },
);

MemoizedMarkdown.displayName = "MemoizedMarkdown";
