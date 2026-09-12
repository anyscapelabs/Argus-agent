import { FiChevronDown } from "react-icons/fi";
import { LuSearch } from "react-icons/lu";

import SkillCard, { type Skill } from "./SkillCard";

const SKILLS: Skill[] = [
  {
    id: "email-triage",
    name: "Email Triage",
    description: "Sort & prioritize your inbox",
  },
  {
    id: "meeting-notes",
    name: "Meeting Notes",
    description: "Auto-generate meeting summaries",
  },
  {
    id: "research",
    name: "Research",
    description: "Deep dive on any topic",
  },
  {
    id: "writing",
    name: "Writing Assistant",
    description: "Draft emails & documents",
  },
  {
    id: "data-analysis",
    name: "Data Analysis",
    description: "Analyze spreadsheets & data",
  },
  {
    id: "scheduling",
    name: "Scheduling",
    description: "Manage calendar & bookings",
  },
];

type SkillsPageProps = {
  onAddSkill?: () => void;
};

export default function SkillsPage({ onAddSkill }: SkillsPageProps) {
  return (
    <div className="mx-auto w-full max-w-2xl py-4">
      <div className="mb-1 flex items-center justify-between">
        <h1 className="text-2xl font-medium text-text-primary">Skills</h1>
        <button
          type="button"
          onClick={onAddSkill ?? (() => {})}
          className={
            "inline-flex items-center justify-center gap-1 rounded-lg " +
            "bg-accent px-2 py-1 text-xs font-medium text-bg-primary " +
            "transition-opacity hover:opacity-90 cursor-pointer"
          }
          aria-label="Add skill"
        >
          Add
          <FiChevronDown size={16} />
        </button>
      </div>
      <p className="mb-5 text-sm font-medium text-text-secondary">
        Equip Argus with specialized capabilities.
      </p>
      <div
        className={
          "flex h-10 w-full items-center rounded-full border " +
          "border-border-primary bg-bg-secondary px-4"
        }
      >
        <LuSearch size={18} className="shrink-0 text-text-secondary" />
        <input
          type="text"
          placeholder="Search skills..."
          className={
            "ml-2 flex-1 bg-transparent text-sm text-text-primary " +
            "outline-none placeholder:text-text-secondary"
          }
        />
      </div>
      <div className="mt-6 grid grid-cols-2 gap-4">
        {SKILLS.map((skill) => (
          <SkillCard key={skill.id} skill={skill} />
        ))}
      </div>
    </div>
  );
}
