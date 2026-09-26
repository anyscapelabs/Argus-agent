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
import { Btn, Card, INPUT, Note, Page, Row, Section, Segmented } from "./kit";

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
    <Page>
      <Section label="Default profile" note="Used when a command names none.">
        <Card>
          <Row
            title="Profile"
            desc={PROFILES.find((p) => p.id === profile)?.blurb}
          >
            <Segmented
              value={profile}
              opts={PROFILES.map((p) => ({ value: p.id, label: p.label }))}
              onChange={setProfile}
            />
          </Row>
        </Card>
      </Section>

      <Section label="Network">
        <Card>
          <Row
            stacked
            title="Outbound ports"
            desc="Project profile only."
          >
            <input
              type="text"
              value={ports}
              placeholder="80, 443"
              autoComplete="off"
              onChange={(e) => setPorts(e.target.value)}
              className={INPUT + " font-mono text-xs"}
            />
          </Row>
          <Row
            stacked
            title="Trusted hosts"
            desc="One per line. Clones and fetches from these count as trusted."
          >
            <textarea
              value={hosts}
              rows={4}
              placeholder={"github.com\n"}
              onChange={(e) => setHosts(e.target.value)}
              className={INPUT + " resize-y font-mono text-xs"}
            />
          </Row>
        </Card>
      </Section>

      <Section
        label="Boundary check"
        note="Runs a real command through the boundary and checks it was actually denied what it was not given. A backend that cannot prove this refuses to run isolated commands at all."
      >
        <Card>
          <Row
            title="Run self-test"
            desc={
              probe === null
                ? "Landlock, seccomp and rlimits on Linux, sandbox-exec on macOS, Job Objects on Windows."
                : `${probe.backend} · ${
                    probe.ok ? "boundary confirmed" : "boundary unavailable"
                  }`
            }
          >
            <Btn onClick={() => void check()} disabled={checking}>
              {checking ? "Checking…" : "Run"}
            </Btn>
          </Row>
          {probe !== null && !probe.ok && (
            <div className="px-4 py-3.5">
              <Note>{probe.detail}</Note>
            </div>
          )}
        </Card>
      </Section>

      <Section label="Recent runs" note="Command metadata and byte counts only — output is never written here.">
        <Card>
          {runs.length === 0 ? (
            <div className="px-4 py-3.5">
              <Note>Nothing has run yet.</Note>
            </div>
          ) : (
            runs.map((r) => (
              <div key={r.id} className="flex items-baseline gap-2 px-4 py-2.5">
                <span className="font-mono text-xs text-text-tertiary">
                  {r.profile}
                </span>
                <span className="min-w-0 flex-1 truncate font-mono text-xs text-text-primary">
                  {r.command}
                </span>
                <span className="shrink-0 font-mono text-xs text-text-tertiary">
                  exit {r.exit} · {r.termination} ·{" "}
                  {(r.durationMs / 1000).toFixed(1)}s
                  {r.truncated ? " · truncated" : ""}
                </span>
              </div>
            ))
          )}
        </Card>
      </Section>

      <div className="flex items-center gap-3">
        <span className="min-w-0 flex-1">
          {err !== null && <span className="text-xs text-red-400">{err}</span>}
          {err === null && note !== "" && <Note>{note}</Note>}
        </span>
        <Btn variant="primary" onClick={() => void save()} disabled={busy}>
          {busy ? "Saving…" : "Save"}
        </Btn>
      </div>
    </Page>
  );
}
