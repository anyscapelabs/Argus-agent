import type { ReactNode } from "react";

export type LibraryItem = {
  id: string;
  name: string;
  description: string;
  icon: ReactNode;
};

type LibraryCardProps = {
  item: LibraryItem;
};

export default function LibraryCard({ item }: LibraryCardProps) {
  return (
    <div className="flex items-center gap-4 px-2 py-1 bg-transparent rounded-xl hover:bg-bg-hover-primary cursor-pointer transition-colors">
      <div className="flex items-center justify-center w-12 h-12 shrink-0 bg-bg-primary border border-border-primary rounded-lg text-text-primary text-xl">
        {item.icon}
      </div>

      <div className="flex-1 min-w-0">
        <h3 className="text-sm font-medium text-text-primary truncate">
          {item.name}
        </h3>
        <p className="text-xs text-text-secondary truncate">
          {item.description}
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
