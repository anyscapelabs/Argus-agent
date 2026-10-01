import type { BlockNode } from "../../lib/agentXml";
import { MemoizedMarkdown } from "./MemoizedMarkdown";

type Props = { block: BlockNode };

export default function AlertBanner({ block }: Props) {
  const body = block.children.map((c) => c.value).join("").trim();
  if (!body) return null;

  // Bullets are lines. Splitting sentences multiplied bullets and hid markup.
  const items = body
    .split("\n")
    .map((s) => s.trim())
    .filter(Boolean);
  const bullets = items.length ? items : [body];

  return (
    <div className="mt-4 first:mt-0">
      <div className="font-sans text-[16px] font-medium text-text-primary">
        Things to note:
      </div>
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
