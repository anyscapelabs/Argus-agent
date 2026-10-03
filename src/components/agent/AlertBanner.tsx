import type { BlockNode } from "../../lib/agentXml";
import { MemoizedMarkdown } from "./MemoizedMarkdown";

type Props = { block: BlockNode };

const KIND_HEAD: Record<string, { title: string; hint: string }> = {
  auth: {
    title: "Authentication failed",
    hint: "Check the API key in Providers settings, then retry.",
  },
  quota: {
    title: "Out of quota",
    hint: "Switch to another model or wait for the limit to reset.",
  },
  server: {
    title: "Provider error",
    hint: "The provider failed the request — retry in a moment.",
  },
  network: {
    title: "Connection failed",
    hint: "Check the network connection, then retry.",
  },
  config: {
    title: "Setup needed",
    hint: "Connect a provider that serves this model, then retry.",
  },
};

export default function AlertBanner({ block }: Props) {
  const body = block.children.map((c) => c.value).join("").trim();
  if (!body) return null;

  // Bullets are lines. Splitting sentences multiplied bullets and hid markup.
  const items = body
    .split("\n")
    .map((s) => s.trim())
    .filter(Boolean);
  const bullets = items.length ? items : [body];
  const head =
    block.tag === "error" ? KIND_HEAD[block.attrs.kind ?? ""] : undefined;

  return (
    <div className="mt-4 first:mt-0">
      <div className="font-sans text-[16px] font-medium text-text-primary">
        {head ? head.title : "Things to note:"}
      </div>
      {head && (
        <p className="mt-0.5 font-sans text-sm text-text-secondary">
          {head.hint}
        </p>
      )}
      <ul className="mt-1.5 ml-4 flex list-disc flex-col gap-1 marker:text-text-tertiary">
        {bullets.map((it, i) => (
          <li
            key={i}
            className="font-serif text-[16px] font-light leading-6 text-text-secondary"
          >
            <MemoizedMarkdown content={it} id={`warn-${i}`} />
          </li>
        ))}
      </ul>
    </div>
  );
}
