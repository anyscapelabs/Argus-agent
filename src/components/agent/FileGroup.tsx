import { useState } from "react";
import { FiChevronDown } from "react-icons/fi";
import type { BlockNode } from "../../lib/agentXml";

type Props = {
  blocks: BlockNode[];
  diffs: Map<string, { added: number; removed: number }>;
};

export default function FileGroup({ blocks, diffs }: Props) {
  const [open, setOpen] = useState(false);
  const files = blocks.map((b) => ({ path: b.attrs.path ?? "file", action: b.attrs.action ?? "edited", type: b.attrs.type }));
  return (
    <div className="mt-4 font-sans first:mt-0">
      <button type="button" onClick={() => setOpen((v) => !v)} className="flex w-fit items-center gap-1.5 text-sm text-text-secondary transition-colors hover:text-text-primary focus:outline-none focus-visible:text-text-primary" aria-expanded={open}>
        <span className="text-sm">Files Edited</span>
        <span className="text-sm text-text-secondary">{files.length}</span>
        <FiChevronDown size={14} className={`shrink-0 transition-transform ${open ? "rotate-0" : "-rotate-90"}`} />
      </button>
      {open && (
        <ul className="mt-1.5 flex flex-col gap-0.5">
          {files.map((f, idx) => {
            const name = f.path.split("/").pop() ?? f.path;
            const st = diffs.get(f.path);
            const isNew = f.action === "created";
            const add = st ? st.added : isNew ? 140 : null;
            const rm = st ? st.removed : null;
            const has_diff = add !== null;
            return (
              <li key={`${f.path}-${idx}`} className="flex cursor-pointer items-center gap-3 rounded px-1 py-1 text-sm transition-colors hover:bg-bg-secondary">
                <span className="truncate font-mono text-sm text-text-secondary">{name}</span>
                <span className="text-sm text-text-secondary">{f.action}</span>
                {has_diff && (
                  <span className="ml-auto flex shrink-0 items-center gap-1.5 font-mono text-xs">
                    <span className="text-emerald-400">+{add}</span>
                    {rm !== null && rm > 0 && <span className="text-red-400">-{rm}</span>}
                  </span>
                )}
              </li>
            );
          })}
        </ul>
      )}
    </div>
  );
}
