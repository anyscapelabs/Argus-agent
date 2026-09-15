import { openPath } from "@tauri-apps/plugin-opener";
import { useEffect, useState } from "react";
import {
  LuFileText,
  LuPresentation,
  LuSearch,
  LuSheet,
  LuVideo,
} from "react-icons/lu";

import type { LibItem } from "../lib/ipc";
import { libraryList, libraryPath, librarySearch } from "../lib/ipc";
import LibraryCard from "./LibraryCard";

function iconFor(item: LibItem) {
  if (item.kind === "presentation") return <LuPresentation />;
  if (item.kind === "sheet") return <LuSheet />;
  if (item.kind === "video") return <LuVideo />;
  return <LuFileText />;
}

function describe(item: LibItem) {
  return `${item.ext.toUpperCase()} · ${(item.sz / 1024).toFixed(1)} KB`;
}

export default function LibraryPage() {
  const [items, setItems] = useState<LibItem[]>([]);
  const [query, setQuery] = useState("");
  const [err, setErr] = useState<string | null>(null);

  useEffect(() => {
    let alive = true;
    libraryList()
      .then((rows) => {
        if (alive) {
          setItems(rows.filter((r) => r.kind !== "image"));
          setErr(null);
        }
      })
      .catch((e) => {
        if (alive) setErr(String(e));
      });
    return () => {
      alive = false;
    };
  }, []);

  useEffect(() => {
    if (query.trim() === "") {
      libraryList()
        .then((rows) => setItems(rows.filter((r) => r.kind !== "image")))
        .catch(() => {});
      return;
    }
    const t = setTimeout(() => {
      librarySearch(query.trim())
        .then((rows) => setItems(rows.filter((r) => r.kind !== "image")))
        .catch(() => {});
    }, 200);
    return () => clearTimeout(t);
  }, [query]);

  const open = async (id: string) => {
    try {
      const abs = await libraryPath(id);
      await openPath(abs);
    } catch {}
  };

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
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder="Search library..."
          className={
            "ml-2 flex-1 bg-transparent text-sm text-text-primary " +
            "outline-none placeholder:text-text-secondary"
          }
        />
      </div>
      {err !== null && (
        <p className="mt-4 text-sm text-red-400">{err}</p>
      )}
      <div className="mt-6 grid grid-cols-2 gap-4">
        {items.map((item) => (
          <div key={item.id} onClick={() => open(item.id)}>
            <LibraryCard
              item={{
                id: item.id,
                name: item.name,
                description: describe(item),
                icon: iconFor(item),
              }}
            />
          </div>
        ))}
      </div>
      {err === null && items.length === 0 && (
        <p className="mt-6 text-center text-sm text-text-secondary">
          No documents yet. Ask Argus to create a report, deck or sheet.
        </p>
      )}
    </div>
  );
}
