import {
  LuFileText,
  LuImage,
  LuPresentation,
  LuSearch,
  LuSheet,
  LuVideo,
} from "react-icons/lu";

import LibraryCard, { type LibraryItem } from "./LibraryCard";

const LIBRARY_ITEMS: LibraryItem[] = [
  {
    id: "report-q4",
    name: "Q4 Report.pdf",
    description: "Generated 2 hours ago",
    icon: <LuFileText />,
  },
  {
    id: "brand-hero",
    name: "Brand Hero.png",
    description: "Generated yesterday",
    icon: <LuImage />,
  },
  {
    id: "pitch-deck",
    name: "Pitch Deck.pptx",
    description: "Generated 3 days ago",
    icon: <LuPresentation />,
  },
  {
    id: "budget-sheet",
    name: "Budget Sheet.xlsx",
    description: "Generated 3 days ago",
    icon: <LuSheet />,
  },
  {
    id: "demo-video",
    name: "Demo Video.mp4",
    description: "Generated 5 days ago",
    icon: <LuVideo />,
  },
  {
    id: "research-doc",
    name: "Research Notes.md",
    description: "Generated 1 week ago",
    icon: <LuFileText />,
  },
];

export default function LibraryPage() {
  return (
    <div className="mx-auto w-full max-w-2xl py-4">
      <h1 className="mb-1 text-2xl font-medium text-text-primary">
        Library
      </h1>
      <p className="mb-5 text-sm font-medium text-text-secondary">
        All documents and images created by your agent.
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
          placeholder="Search library..."
          className={
            "ml-2 flex-1 bg-transparent text-sm text-text-primary " +
            "outline-none placeholder:text-text-secondary"
          }
        />
      </div>
      <div className="mt-6 grid grid-cols-2 gap-4">
        {LIBRARY_ITEMS.map((item) => (
          <LibraryCard key={item.id} item={item} />
        ))}
      </div>
    </div>
  );
}
