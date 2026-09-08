import { FiChevronDown } from "react-icons/fi";
import { LuSearch } from "react-icons/lu";

import ProjectCard, { type Project } from "./ProjectCard";

const projects: Project[] = [
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
      <div className="flex items-center justify-between mb-1">
        <h1 className="text-2xl text-text-primary font-medium">Projects</h1>
        <button
          type="button"
          onClick={onNewProject ?? (() => console.log("new project"))}
          className="inline-flex items-center justify-center gap-1 rounded-lg bg-accent px-2 py-1 text-xs font-medium text-bg-primary hover:opacity-90 transition-opacity cursor-pointer"
          aria-label="New project"
        >
          New
          <FiChevronDown size={16} />
        </button>
      </div>
      <p className="text-sm text-text-secondary font-medium mb-5">
        Group related chats, files, and skills under one workspace.
      </p>

      <div className="flex items-center w-full h-10 px-4 bg-bg-secondary border border-border-primary rounded-full">
        <LuSearch size={18} className="text-text-secondary shrink-0" />
        <input
          type="text"
          placeholder="Search projects..."
          className="flex-1 ml-2 bg-transparent outline-none text-sm text-text-primary placeholder:text-text-secondary"
        />
      </div>

      <div className="mt-6 grid grid-cols-2 gap-4">
        {projects.map((project) => (
          <ProjectCard key={project.id} project={project} />
        ))}
      </div>
    </div>
  );
}
