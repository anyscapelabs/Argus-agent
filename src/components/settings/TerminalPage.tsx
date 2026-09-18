import { useCallback, useEffect, useState } from "react";

import {
  termShellClear,
  termShellSet,
  termShellStatus,
  type TermShellStatus,
} from "../../lib/ipc";

export default function TerminalPage() {
  const [status, setStatus] = useState<TermShellStatus | null>(null);
  const [draft, setDraft] = useState("");
  const [err, setErr] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      const next = await termShellStatus();
      setStatus(next);
      setErr(null);
    } catch (e) {
      setErr(e instanceof Error ? e.message : "shell status failed");
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  async function onSave() {
    const path = draft.trim();

    if (!path) return;

    try {
      const next = await termShellSet(path);
      setStatus(next);
      setDraft("");
      setErr(null);
    } catch (e) {
      setErr(e instanceof Error ? e.message : "shell override failed");
    }
  }

  async function onClear() {
    try {
      const next = await termShellClear();
      setStatus(next);
      setDraft("");
      setErr(null);
    } catch (e) {
      setErr(e instanceof Error ? e.message : "shell reset failed");
    }
  }

  return (
    <div className="flex flex-col gap-4 px-2 py-3">
      <div>
        <h2 className="text-sm font-medium text-text-primary">Shell</h2>
        <p className="text-xs text-text-secondary">
          Auto-detected once at startup. Override only if detection guesses
          wrong.
        </p>
      </div>
      <div className="flex flex-col gap-1 text-xs">
        <div className="flex gap-2">
          <span className="w-20 shrink-0 text-text-secondary">Binary</span>
          <span className="truncate font-mono text-text-primary">
            {status?.binary ?? "…"}
          </span>
        </div>
        <div className="flex gap-2">
          <span className="w-20 shrink-0 text-text-secondary">Kind</span>
          <span className="text-text-primary">{status?.kind ?? "…"}</span>
        </div>
        <div className="flex gap-2">
          <span className="w-20 shrink-0 text-text-secondary">Version</span>
          <span className="truncate font-mono text-text-primary">
            {status?.version ?? "unknown"}
          </span>
        </div>
        <div className="flex gap-2">
          <span className="w-20 shrink-0 text-text-secondary">Source</span>
          <span className="text-text-primary">{status?.source ?? "…"}</span>
        </div>
      </div>
      <div className="flex gap-2">
        <input
          value={draft}
          onChange={(evt) => setDraft(evt.target.value)}
          placeholder="/usr/bin/bash"
          spellCheck={false}
          className={
            "h-8 min-w-0 flex-1 rounded-lg border border-border-primary " +
            "bg-bg-primary px-3 font-mono text-xs text-text-primary " +
            "outline-none placeholder:text-text-secondary"
          }
        />
        <button
          type="button"
          onClick={() => void onSave()}
          className={
            "h-8 shrink-0 rounded-lg bg-accent px-3 text-xs font-medium " +
            "text-white"
          }
        >
          Save
        </button>
        <button
          type="button"
          onClick={() => void onClear()}
          className={
            "h-8 shrink-0 rounded-lg border border-border-primary px-3 " +
            "text-xs font-medium text-text-secondary hover:text-text-primary"
          }
        >
          Auto
        </button>
      </div>
      {err && <p className="text-xs text-red-400">{err}</p>}
    </div>
  );
}
