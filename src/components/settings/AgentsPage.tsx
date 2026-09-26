import { useEffect, useState } from "react";

import { agentKeep, agentSetKeep } from "../../lib/ipc";

const KEEPS = [
  { value: 0, label: "All" },
  { value: 50, label: "50" },
  { value: 200, label: "200" },
  { value: 1000, label: "1000" },
];

const BLURBS: Record<number, string> = {
  0: "Keep every transcript. Disk grows with every sub-agent you start.",
  50: "The last 50 per chat. Older ones and their transcripts are deleted at the next start.",
  200: "The last 200 per chat. Older ones and their transcripts are deleted at the next start.",
  1000: "The last 1000 per chat. Older ones and their transcripts are deleted at the next start.",
};

export default function AgentsPage() {
  const [keep, setKeep] = useState<number | null>(null);

  useEffect(() => {
    void agentKeep()
      .then(setKeep)
      .catch(() => setKeep(200));
  }, []);

  if (keep === null) return null;

  async function pick(n: number) {
    const prev = keep;
    setKeep(n);

    try {
      setKeep(await agentSetKeep(n));
    } catch {
      setKeep(prev);
    }
  }

  return (
    <div className="flex flex-col gap-2 px-2 py-3">
      <div>
        <h2 className="text-sm font-medium text-text-primary">Sub-agents</h2>
        <p className="text-xs text-text-secondary">
          Each sub-agent keeps its own transcript, opened from its card in the
          chat. Nothing else ever reads it, so it is safe to let old ones go.
        </p>
      </div>

      <div className="mt-1 flex flex-col gap-1.5">
        {KEEPS.map((opt) => {
          const active = opt.value === keep;

          return (
            <button
              key={opt.value}
              type="button"
              onClick={() => pick(opt.value)}
              aria-pressed={active}
              className={
                "flex items-center gap-3 rounded-lg border px-3 py-2.5 " +
                "text-left transition-colors cursor-pointer " +
                (active
                  ? "border-accent bg-bg-hover-secondary"
                  : "border-border-primary hover:bg-bg-hover-primary")
              }
            >
              <div className="min-w-0 flex-1">
                <div className="text-sm text-text-primary">
                  {opt.value === 0 ? "Keep everything" : `Keep the last ${opt.label}`}
                </div>
                <div className="text-xs text-text-secondary">
                  {BLURBS[opt.value]}
                </div>
              </div>
            </button>
          );
        })}
      </div>

      <p className="mt-1 text-xs text-text-tertiary">
        The prune runs once, when Argus starts. A sub-agent that is still
        running is never one of the ones removed.
      </p>
    </div>
  );
}
