import { LuWand } from "react-icons/lu";

import type { Skill } from "../lib/ipc";
import { formatRelativeTime, parseDbTime } from "../lib/relativeTime";

type SkillCardProps = {
  skill: Skill;
  onOpen: (skill: Skill) => void;
};

export default function SkillCard({ skill, onOpen }: SkillCardProps) {
  const created = parseDbTime(skill.created_at);
  const from = skill.origin ? `from ${skill.origin} · ` : "";

  return (
    <div
      onClick={() => onOpen(skill)}
      className={
        "flex cursor-pointer items-center gap-3 bg-transparent " +
        "px-3 py-2.5 transition-colors hover:bg-bg-hover-primary"
      }
    >
      <div
        className={
          "flex h-10 w-10 shrink-0 items-center justify-center rounded-lg " +
          "border border-border-primary bg-bg-primary text-text-primary"
        }
      >
        <LuWand size={16} />
      </div>
      <div className="min-w-0 flex-1">
        <div className="flex items-center gap-2">
          <h3 className="truncate text-sm font-medium text-text-primary">
            {skill.name}
          </h3>
        </div>
        <p className="truncate text-xs text-text-secondary">
          {from}
          {skill.description}
        </p>
      </div>
      {created !== null && (
        <span className="shrink-0 text-xs text-text-secondary">
          {formatRelativeTime(created)}
        </span>
      )}
    </div>
  );
}
