import { useEffect, useState } from "react";
import { FiSearch } from "react-icons/fi";
import { HiArrowLeft } from "react-icons/hi2";

import { libraryList, librarySearch, type LibItem } from "../lib/ipc";
import { attachStore } from "../stores/attachments";

const KIND_LABEL: Record<string, string> = {
  image: "Images",
  doc: "Documents",
  sheet: "Sheets",
  presentation: "Decks",
  file: "Files",
};

export default function LibraryPanel({
  onBack,
}: {
  onBack: () => void;
}) {
  const [items, setItems] = useState<LibItem[] | null>(null);
  const [query, setQuery] = useState("");
  const [err, setErr] = useState<string | null>(null);

  useEffect(() => {
    let live = true;

    const load = async () => {
      try {
        const found = query.trim()
          ? await librarySearch(query.trim())
          : await libraryList();
        if (live) {
          setItems(found);
          setErr(null);
        }
      } catch (e) {
        if (live) {
          setErr(e instanceof Error ? e.message : String(e));
        }
      }
    };

    const timer = setTimeout(load, query.trim() ? 180 : 0);
    return () => {
      live = false;
      clearTimeout(timer);
    };
  }, [query]);

  return (
    <div className="w-[300px]">
      <div className="flex items-center gap-1 border-b border-border-primary px-2 py-1.5">
        <button
          type="button"
          aria-label="Back"
          onClick={onBack}
          className="shrink-0 rounded p-1 text-text-secondary hover:bg-bg-hover-secondary hover:text-text-primary"
        >
          <HiArrowLeft size={14} />
        </button>

        <div className="flex flex-1 items-center gap-1.5 rounded-lg bg-bg-tertiary px-2">
          <FiSearch size={13} className="shrink-0 text-text-tertiary" />
          <input
            autoFocus
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="Search the library"
            className="w-full bg-transparent py-1 text-xs text-text-primary placeholder:text-text-tertiary focus:outline-none"
          />
        </div>
      </div>

      <div className="max-h-[260px] overflow-y-auto p-1">
        {err !== null && (
          <p className="px-2 py-1.5 text-xs text-text-secondary">{err}</p>
        )}

        {items === null && err === null && (
          <p className="px-2 py-1.5 text-xs text-text-secondary">Loading…</p>
        )}

        {items !== null && items.length === 0 && (
          <p className="px-2 py-1.5 text-xs text-text-secondary">
            {query.trim() ? "Nothing matched." : "The library is empty."}
          </p>
        )}

        {items?.map((item) => (
          <button
            key={item.id}
            type="button"
            onClick={() => void attachStore.addLibItems([item])}
            className="flex w-full items-center gap-2 rounded-xl px-2 py-1.5 text-left transition-colors hover:bg-bg-hover-secondary focus:outline-none focus-visible:bg-bg-hover-secondary"
          >
            <span className="shrink-0 text-[10px] uppercase text-text-tertiary">
              {item.ext}
            </span>
            <span className="flex-1 truncate text-sm text-text-primary">
              {item.name}
            </span>
            <span className="shrink-0 text-[10px] text-text-tertiary">
              {KIND_LABEL[item.kind] ?? item.kind}
            </span>
          </button>
        ))}
      </div>
    </div>
  );
}
