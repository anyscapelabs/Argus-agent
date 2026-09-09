import { FiChevronDown } from "react-icons/fi";
import { LuSearch } from "react-icons/lu";

import ProjectCard, { type Project } from "./ProjectCard";

const PROJECTS: Project[] = [
  {
    id: "launch-website",
    name: "Launch Website",
    description: "Copy, assets, and tasks for the v1 launch",
  },
  {
    id: "q4-report",
    name: "Q4 Report",
    description: "Spreadsheets, drafts, and charts for the quarterly review",
  },
  {
    id: "research-notebook",
    name: "Research Notebook",
    description: "Sources, notes, and follow-ups from ongoing research",
  },
  {
    id: "brand-refresh",
    name: "Brand Refresh",
    description: "Logos, guidelines, and copy experiments",
  },
  {
    id: "investor-updates",
    name: "Investor Updates",
    description: "Monthly updates, metrics, and drafts",
  },
  {
    id: "trip-japan",
    name: "Trip: Japan",
    description: "Itinerary, bookings, and reservation confirmations",
  },
];

type ProjectsPageProps = {
  onNewProject?: () => void;
};

export default function ProjectsPage({ onNewProject }: ProjectsPageProps) {
  return (
    <div className="mx-auto w-full max-w-2xl py-4">
      <div className="mb-1 flex items-center justify-between">
        <h1 className="text-2xl font-medium text-text-primary">Projects</h1>
        <button
          type="button"
          onClick={onNewProject ?? (() => console.log("new project"))}
          className={
            "inline-flex items-center justify-center gap-1 rounded-lg " +
            "bg-accent px-2 py-1 text-xs font-medium text-bg-primary " +
            "transition-opacity hover:opacity-90 cursor-pointer"
          }
          aria-label="New project"
        >
          New
          <FiChevronDown size={16} />
        </button>
      </div>
      <p className="mb-5 text-sm font-medium text-text-secondary">
        Group related chats, files, and skills under one workspace.
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
          placeholder="Search projects..."
          className={
            "ml-2 flex-1 bg-transparent text-sm text-text-primary " +
            "outline-none placeholder:text-text-secondary"
          }
        />
      </div>
      <div className="mt-6 grid grid-cols-2 gap-4">
        {PROJECTS.map((project) => (
          <ProjectCard key={project.id} project={project} />
        ))}
      </div>
    </div>
  );
}
