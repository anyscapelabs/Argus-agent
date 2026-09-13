import { useState } from "react";
import { FiTrash2 } from "react-icons/fi";

import type { Skill } from "../lib/ipc";
import { skillGet } from "../lib/ipc";

type SkillCardProps = {
  skill: Skill;
  onDelete: (name: string) => void;
};

export default function SkillCard({ skill, onDelete }: SkillCardProps) {
  const [open, setOpen] = useState(false);
  const [body, setBody] = useState<string | null>(null);

  const toggle = async () => {
    if (open) {
      setOpen(false);
      return;
    }

    try {
      const full = await skillGet(skill.name);
      setBody(full.body);
    } catch {
      setBody(skill.description);
    }

    setOpen(true);
  };

  return (
    <div
      className={
        "flex flex-col gap-2 rounded-xl bg-transparent px-2 py-1 " +
        "transition-colors hover:bg-bg-hover-primary"
      }
    >
      <div className="flex items-center gap-4">
        <button
          type="button"
          onClick={toggle}
          aria-expanded={open}
          className="flex-1 min-w-0 text-left focus:outline-none"
        >
          <h3 className="truncate text-sm font-medium text-text-primary">
            {skill.name}
          </h3>
          <p className="truncate text-xs text-text-secondary">
            {skill.description}
          </p>
        </button>
        <span className="shrink-0 rounded-full border border-border-primary px-2 py-0.5 text-[10px] text-text-secondary">
          {skill.source}
        </span>
        <button
          type="button"
          onClick={() => onDelete(skill.name)}
          aria-label={`Delete ${skill.name}`}
          className={
            "flex h-7 w-7 shrink-0 items-center justify-center rounded-md " +
            "text-text-secondary transition-colors hover:bg-bg-hover-primary " +
            "hover:text-text-primary focus:outline-none"
          }
        >
          <FiTrash2 size={14} />
        </button>
      </div>
      {open && body !== null && (
        <pre className="max-h-[240px] overflow-y-auto whitespace-pre-wrap px-1 font-mono text-xs leading-5 text-text-primary">
          {body}
        </pre>
      )}
    </div>
  );
}
