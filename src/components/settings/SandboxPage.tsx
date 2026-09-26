import { useCallback, useEffect, useState } from "react";

import {
  sandboxConfig,
  sandboxSetConfig,
  type SandboxProfile,
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
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  const [note, setNote] = useState("");

  const refresh = useCallback(async () => {
    try {
      const cfg = await sandboxConfig();

      setProfile(cfg.defaultProfile);
      setHosts(cfg.hosts.join("\n"));
      setPorts(cfg.netAllow.join(", "));
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
