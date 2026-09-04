import { useState } from "react";
import { FiChevronDown } from "react-icons/fi";
import type { BlockNode } from "../../lib/agentXml";

type Props = { block?: BlockNode; steps?: BlockNode[] };

export default function PlanBlock({ steps: stepsProp }: Props) {
  const [open, setOpen] = useState(false);
  const steps = stepsProp ?? [];
  const hasSteps = steps.length > 0;
  return (
    <div className="mt-4 font-sans first:mt-0">
      <button type="button" onClick={() => setOpen((v) => !v)} className="flex w-fit items-center gap-1.5 text-[16px] text-text-secondary transition-colors hover:text-text-primary focus:outline-none focus-visible:text-text-primary" aria-expanded={open}>
        <span className="text-[16px]">Steps</span>
        {hasSteps && <span className="text-[16px] text-text-secondary">{steps.length}</span>}
        <FiChevronDown size={14} className={`shrink-0 transition-transform ${open ? "rotate-0" : "-rotate-90"}`} />
      </button>
      {open && (
        <ul className="mt-1.5 ml-4 flex list-disc flex-col gap-1 marker:text-text-secondary/60">
          {hasSteps ? steps.map((s, idx) => {
            const txt = s.children.map((c) => c.value).join("").trim();
            return <li key={s.attrs.id ?? `step-${idx}`} className="font-serif text-[16px] font-light leading-6 text-text-secondary">{txt || "…"}</li>;
          }) : <li className="font-serif text-[16px] italic text-text-secondary">planning…</li>}
        </ul>
      )}
    </div>
  );
}
