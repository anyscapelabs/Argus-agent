import { LuWand } from "react-icons/lu";

import type { Skill } from "../lib/ipc";

type SkillCardProps = {
  skill: Skill;
  onOpen: (skill: Skill) => void;
};

export default function SkillCard({ skill, onOpen }: SkillCardProps) {
  return (
    <div
      onClick={() => onOpen(skill)}
      className={
        "flex cursor-pointer items-center gap-3 rounded-xl bg-transparent " +
        "px-2 py-1 transition-colors hover:bg-bg-hover-primary"
      }
    >
      <div
        className={
          "flex h-11 w-11 shrink-0 items-center justify-center rounded-lg " +
          "border border-border-primary bg-bg-primary text-text-primary"
        }
      >
        <LuWand size={18} />
      </div>
      <div className="min-w-0 flex-1">
        <h3 className="truncate text-sm font-medium text-text-primary">
          {skill.name}
        </h3>
        <p className="truncate text-xs text-text-secondary">
          {skill.description}
        </p>
      </div>
      <span className="shrink-0 rounded-md border border-border-primary px-2 py-0.5 text-[10px] text-text-secondary">
        {skill.source}
      </span>
    </div>
  );
}
