export type Project = {
  id: string;
  name: string;
  description: string;
};

type ProjectCardProps = {
  project: Project;
};

export default function ProjectCard({ project }: ProjectCardProps) {
  return (
    <div className="flex items-center gap-4 px-2 py-1 bg-transparent rounded-xl hover:bg-bg-hover-primary cursor-pointer transition-colors">
      <div className="flex-1 min-w-0">
        <h3 className="text-sm font-medium text-text-primary truncate">
          {project.name}
        </h3>
        <p className="text-xs text-text-secondary truncate">
          {project.description}
        </p>
      </div>

      <button
        type="button"
        className="shrink-0 px-3 py-1.5 text-xs font-medium text-text-primary bg-bg-hover-secondary border border-border-primary rounded-full hover:bg-bg-hover-primary transition-colors cursor-pointer"
      >
        Open
      </button>
    </div>
  );
}
