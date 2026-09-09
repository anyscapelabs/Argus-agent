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
    <div
      className={
        "flex items-center gap-4 rounded-xl bg-transparent px-2 py-1 " +
        "transition-colors hover:bg-bg-hover-primary cursor-pointer"
      }
    >
      <div
        className={
          "flex h-12 w-12 shrink-0 items-center justify-center rounded-lg " +
          "border border-border-primary bg-bg-primary text-xl " +
          "text-text-primary"
        }
      >
        {item.icon}
      </div>
      <div className="flex-1 min-w-0">
        <h3 className="truncate text-sm font-medium text-text-primary">
          {item.name}
        </h3>
        <p className="truncate text-xs text-text-secondary">
          {item.description}
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
        Open
      </button>
    </div>
  );
}
