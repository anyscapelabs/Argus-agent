export type Skill = {
  id: string;
  name: string;
  description: string;
};

type SkillCardProps = {
  skill: Skill;
};

export default function SkillCard({ skill }: SkillCardProps) {
  return (
    <div
      className={
        "flex items-center gap-4 rounded-xl bg-transparent px-2 py-1 " +
        "transition-colors hover:bg-bg-hover-primary cursor-pointer"
      }
    >
      <div className="flex-1 min-w-0">
        <h3 className="truncate text-sm font-medium text-text-primary">
          {skill.name}
        </h3>
        <p className="truncate text-xs text-text-secondary">
          {skill.description}
        </p>
      </div>
      <button
        type="button"
        className={
          "shrink-0 rounded-full border border-border-primary " +
          "bg-bg-hover-secondary px-3 py-1.5 text-xs font-medium " +
          "text-text-primary transition-colors hover:bg-bg-hover-primary " +
          "cursor-pointer"
        }
      >
        Enable
      </button>
    </div>
  );
}
