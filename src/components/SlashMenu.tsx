import { useEffect, useRef, useState } from "react";

import { slashList, type SlashCmd } from "../lib/ipc";

const MAX = 7;

type Props = {
  /// The text after the leading `/`, which is the prefix being filtered on.
  query: string;
  onPick: (cmd: SlashCmd) => void;
  onClose: () => void;
};

export default function SlashMenu({ query, onPick, onClose }: Props) {
  const [all, setAll] = useState<SlashCmd[] | null>(null);
  const [at, setAt] = useState(0);
  const listRef = useRef<HTMLDivElement | null>(null);

  // A dozen rows, fetched once per mount. Filtering is local because it
  // happens on every keystroke and a round trip per keypress is not a menu.
  useEffect(() => {
    let live = true;
    void slashList()
      .then((cmds) => live && setAll(cmds))
      .catch(() => live && setAll([]));
    return () => {
      live = false;
    };
  }, []);

  const hits = (all ?? [])
    .filter((c) => c.name.startsWith(query))
    .slice(0, MAX);

  useEffect(() => {
    setAt(0);
  }, [query]);

  useEffect(() => {
    listRef.current
      ?.querySelector('[data-at="true"]')
      ?.scrollIntoView({ block: "nearest" });
  }, [at]);

  // The caret is in the textarea, not here, so the keys are taken at the
  // document. The menu is a completion list, and a completion list that needs
  // the mouse is not one.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "ArrowDown") {
        e.preventDefault();
        setAt((i) => Math.min(i + 1, hits.length - 1));
        return;
      }

      if (e.key === "ArrowUp") {
        e.preventDefault();
        setAt((i) => Math.max(i - 1, 0));
        return;
      }

      if (e.key === "Enter" || e.key === "Tab") {
        const hit = hits[at];
        if (hit !== undefined) {
          e.preventDefault();
          onPick(hit);
        }
        return;
      }

      if (e.key === "Escape") {
        e.preventDefault();
        onClose();
      }
    };

    document.addEventListener("keydown", onKey, true);
    return () => {
      document.removeEventListener("keydown", onKey, true);
    };
  }, [hits, at, onPick, onClose]);

  if (all === null) {
    return null;
  }

  if (hits.length === 0) {
    return (
      <div className="absolute bottom-full left-0 z-50 mb-2 w-[320px] rounded-2xl border border-border-primary bg-bg-secondary p-2 text-xs text-text-secondary shadow-4xl">
        No command starts with /{query}
      </div>
    );
  }

  return (
    <div
      ref={listRef}
      className="absolute bottom-full left-0 z-50 mb-2 w-[320px] overflow-hidden rounded-2xl border border-border-primary bg-bg-secondary p-1 shadow-4xl"
    >
      {hits.map((c, i) => (
        <button
          key={c.name}
          type="button"
          data-at={i === at}
          onMouseEnter={() => setAt(i)}
          onClick={() => onPick(c)}
          className={
            "flex w-full flex-col items-start gap-0.5 rounded-xl px-2 py-1.5 text-left " +
            "transition-colors focus:outline-none " +
            (i === at ? "bg-bg-hover-secondary" : "")
          }
        >
          <span className="flex w-full items-baseline gap-1.5">
            <span className="text-sm font-medium text-text-primary">
              /{c.name}
            </span>
            {c.arg !== null && (
              <span className="truncate text-xs text-text-tertiary">
                {c.arg}
              </span>
            )}
            {c.kind === "prompt" && (
              <span className="ml-auto shrink-0 text-[10px] uppercase text-text-tertiary">
                prompt
              </span>
            )}
          </span>
          <span className="w-full truncate text-xs text-text-secondary">
            {c.desc}
          </span>
        </button>
      ))}
    </div>
  );
}
