import { useCallback, useEffect, useState } from "react";

import {
  sandboxConfig,
  sandboxRuns,
  sandboxSelftest,
  sandboxSetConfig,
  type SandboxProfile,
  type SandboxProbe,
  type SandboxRun,
} from "../../lib/ipc";

const INPUT =
  "w-full rounded-lg border border-border-primary bg-bg-hover-secondary " +
  "px-2 py-1.5 font-mono text-xs text-text-primary " +
  "placeholder:text-text-secondary focus:outline-none " +
  "focus:ring-1 focus:ring-text-secondary";

const PROFILES: { id: SandboxProfile; label: string; blurb: string }[] = [
  {
    id: "restricted",
    label: "Restricted",
    blurb: "No network. Writes only in its own scratch directory. 120s, 2 GB.",
  },
  {
    id: "project",
    label: "Project",
    blurb: "Reads and writes the project. Outbound 80/443 only. 900s, 4 GB.",
  },
  {
    id: "host",
    label: "Host",
    blurb:
      "No isolation — full filesystem, full network. Only pick this for work you trust.",
  },
];

function toPorts(text: string): number[] {
  return text
    .split(/[\s,]+/)
    .map((s) => s.trim())
    .filter((s) => s !== "")
    .map((s) => Number(s))
    .filter((n) => Number.isInteger(n) && n > 0 && n < 65536);
}

export default function SandboxPage() {
  const [profile, setProfile] = useState<SandboxProfile>("restricted");
  const [hosts, setHosts] = useState("");
  const [ports, setPorts] = useState("");
  const [runs, setRuns] = useState<SandboxRun[]>([]);
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  const [note, setNote] = useState("");
  const [probe, setProbe] = useState<SandboxProbe | null>(null);
  const [checking, setChecking] = useState(false);

  const check = async () => {
    setChecking(true);
    setProbe(null);

    try {
      setProbe(await sandboxSelftest());
    } catch (e) {
      setProbe({
        ok: false,
        backend: "unknown",
        enforcing: false,
        detail: e instanceof Error ? e.message : String(e),
      });
    } finally {
      setChecking(false);
    }
  };

  const refresh = useCallback(async () => {
    try {
      const [cfg, history] = await Promise.all([sandboxConfig(), sandboxRuns(25)]);

      setProfile(cfg.defaultProfile);
      setHosts(cfg.hosts.join("\n"));
      setPorts(cfg.netAllow.join(", "));
      setRuns(history);
      setErr(null);
    } catch (e) {
      setErr(e instanceof Error ? e.message : "sandbox config failed");
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const save = async () => {
    setBusy(true);
    setErr(null);
    setNote("");

    try {
      await sandboxSetConfig({
        hosts: hosts.split("\n").map((s) => s.trim()).filter((s) => s !== ""),
        defaultProfile: profile,
        netAllow: toPorts(ports),
      });

      const next = await sandboxConfig();

      setPorts(next.netAllow.join(", "));
      setNote("Saved.");
    } catch (e) {
      setErr(e instanceof Error ? e.message : "save failed");
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="flex flex-col gap-5 px-2 py-3">
      <div>
        <h2 className="text-sm font-medium text-text-primary">Sandbox</h2>
        <p className="text-xs text-text-secondary">
          Isolated commands run under a boundary the OS enforces directly —
          Landlock, seccomp and rlimits on Linux, sandbox-exec on macOS, Job
          Objects on Windows. No container, no image. If a profile cannot be
          enforced the command is refused, never run unconfined.
        </p>
      </div>

      <div className="flex flex-col gap-2">
        <span className="text-sm text-text-secondary">
          Default profile{" "}
          <span className="text-text-tertiary">(used when a command names none)</span>
        </span>

        {PROFILES.map((p) => (
          <button
            key={p.id}
            type="button"
            onClick={() => setProfile(p.id)}
            className={
              "rounded-lg border px-3 py-2 text-left transition-colors " +
              (profile === p.id
                ? "border-accent bg-bg-hover-secondary"
                : "border-border-primary hover:bg-bg-hover-secondary")
            }
          >
            <div className="text-sm text-text-primary">{p.label}</div>
            <div className="text-xs text-text-secondary">{p.blurb}</div>
          </button>
        ))}
      </div>

      <label className="flex flex-col gap-1.5">
        <span className="text-sm text-text-secondary">
          Outbound ports{" "}
          <span className="text-text-tertiary">(Project profile only)</span>
        </span>
        <input
          type="text"
          value={ports}
          placeholder="80, 443"
          autoComplete="off"
          onChange={(e) => setPorts(e.target.value)}
          className={INPUT}
        />
      </label>

      <label className="flex flex-col gap-1.5">
        <span className="text-sm text-text-secondary">
          Trusted hosts{" "}
          <span className="text-text-tertiary">
            (one per line — clones and fetches from these count as trusted)
          </span>
        </span>
        <textarea
          value={hosts}
          rows={3}
          placeholder={"github.com\n"}
          onChange={(e) => setHosts(e.target.value)}
          className={INPUT + " resize-y"}
        />
      </label>

      {err && <p className="text-xs text-red-400">{err}</p>}
      {note && <p className="text-xs text-green-500">{note}</p>}

      <div>
        <button
          type="button"
          onClick={save}
          disabled={busy}
          className={
            "rounded-md bg-accent px-3 py-1.5 text-sm font-medium " +
            "text-bg-primary transition-opacity hover:opacity-90 " +
            "disabled:opacity-50"
          }
        >
          {busy ? "Saving…" : "Save"}
        </button>
      </div>

      <div className="flex flex-col gap-2">
        <h3 className="text-sm font-medium text-text-primary">Boundary check</h3>

        <p className="text-xs text-text-secondary">
          Runs a real command through the boundary and checks it was actually
          denied what it was not given. A backend that cannot prove this
          refuses to run isolated commands at all.
        </p>

        <div>
          <button
            type="button"
            onClick={check}
            disabled={checking}
            className={
              "rounded-md border border-border-primary px-3 py-1.5 text-sm " +
              "text-text-primary transition-colors hover:bg-bg-hover-secondary " +
              "disabled:opacity-50"
            }
          >
            {checking ? "Checking…" : "Run self-test"}
          </button>
        </div>

        {probe && (
          <div
            className={
              "rounded-md border px-2 py-1.5 text-xs " +
              (probe.ok
                ? "border-green-500/30 text-green-500"
                : "border-red-500/30 text-red-400")
            }
          >
            <div className="font-mono">
              {probe.backend} ·{" "}
              {probe.ok ? "boundary confirmed" : "boundary unavailable"}
            </div>
            <div className="text-text-secondary">{probe.detail}</div>
          </div>
        )}
      </div>

      <div className="flex flex-col gap-2">
        <h3 className="text-sm font-medium text-text-primary">Recent runs</h3>

        {runs.length === 0 ? (
          <p className="text-xs text-text-secondary">Nothing has run yet.</p>
        ) : (
          <div className="flex flex-col gap-1">
            {runs.map((r) => (
              <div
                key={r.id}
                className="flex items-baseline gap-2 rounded-md bg-bg-hover-secondary px-2 py-1"
              >
                <span className="font-mono text-xs text-text-secondary">
                  {r.profile}
                </span>
                <span className="truncate font-mono text-xs text-text-primary">
                  {r.command}
                </span>
                <span className="ml-auto shrink-0 text-xs text-text-secondary">
                  exit {r.exit} · {r.termination} ·{" "}
                  {(r.durationMs / 1000).toFixed(1)}s
                  {r.truncated ? " · truncated" : ""}
                </span>
              </div>
            ))}
          </div>
        )}

        <p className="text-xs text-text-tertiary">
          Command metadata and byte counts only — output is never written here.
        </p>
      </div>
    </div>
  );
}
