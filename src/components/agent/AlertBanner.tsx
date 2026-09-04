import type { BlockNode } from "../../lib/agentXml";

type Props = { block: BlockNode };

export default function AlertBanner({ block }: Props) {
  const body = block.children.map((c) => c.value).join("").trim();
  if (!body) return null;
  const items = body.split("\n").map((s) => s.trim()).filter(Boolean).flatMap((s) => s.split(/(?<=[.!?])\s+/).filter(Boolean));
  const bullets = items.length ? items : [body];
  return (
    <div className="mt-4 first:mt-0">
      <div className="font-sans text-[16px] font-medium text-text-primary">Things to note:</div>
      <ul className="mt-1.5 ml-4 flex list-disc flex-col gap-1 marker:text-text-secondary/60">
        {bullets.map((it, i) => <li key={i} className="font-serif text-[16px] font-light leading-6 text-text-secondary">{it}</li>)}
      </ul>
    </div>
  );
}
