import { FiChevronDown } from "react-icons/fi";
import { LuSearch } from "react-icons/lu";

import SkillCard, { type Skill } from "./SkillCard";

const skills: Skill[] = [
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
      <div className="flex items-center justify-between mb-1">
        <h1 className="text-2xl text-text-primary font-medium">Skills</h1>
        <button
          type="button"
          onClick={onAddSkill ?? (() => console.log("add skill"))}
          className="inline-flex items-center justify-center gap-1 rounded-lg bg-accent px-2 py-1 text-xs font-medium text-bg-primary hover:opacity-90 transition-opacity cursor-pointer"
          aria-label="Add skill"
        >
          Add
          <FiChevronDown size={16} />
        </button>
      </div>
      <p className="text-sm text-text-secondary font-medium mb-5">
        Equip Argus with specialized capabilities.
      </p>

      <div className="flex items-center w-full h-10 px-4 bg-bg-secondary border border-border-primary rounded-full">
        <LuSearch size={18} className="text-text-secondary shrink-0" />
        <input
          type="text"
          placeholder="Search skills..."
          className="flex-1 ml-2 bg-transparent outline-none text-sm text-text-primary placeholder:text-text-secondary"
        />
      </div>

      <div className="mt-6 grid grid-cols-2 gap-4">
        {skills.map((skill) => (
          <SkillCard key={skill.id} skill={skill} />
        ))}
      </div>
    </div>
  );
}
