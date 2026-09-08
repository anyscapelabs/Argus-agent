import { LuFileText, LuImage, LuPresentation, LuSheet, LuVideo } from "react-icons/lu";
import { LuSearch } from "react-icons/lu";

import LibraryCard, { type LibraryItem } from "./LibraryCard";

const libraryItems: LibraryItem[] = [
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
      <h1 className="text-2xl text-text-primary font-medium mb-1">Library</h1>
      <p className="text-sm text-text-secondary font-medium mb-5">
        All documents and images created by your agent.
      </p>

      <div className="flex items-center w-full h-10 px-4 bg-bg-secondary border border-border-primary rounded-full">
        <LuSearch size={18} className="text-text-secondary shrink-0" />
        <input
          type="text"
          placeholder="Search library..."
          className="flex-1 ml-2 bg-transparent outline-none text-sm text-text-primary placeholder:text-text-secondary"
        />
      </div>

      <div className="mt-6 grid grid-cols-2 gap-4">
        {libraryItems.map((item) => (
          <LibraryCard key={item.id} item={item} />
        ))}
      </div>
    </div>
  );
}
