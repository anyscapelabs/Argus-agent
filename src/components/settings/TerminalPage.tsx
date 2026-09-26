import { useCallback, useEffect, useState } from "react";

import {
  termShellClear,
  termShellSet,
  termShellStatus,
  type TermShellStatus,
} from "../../lib/ipc";
import { Btn, Card, INPUT, Page, Row, Section } from "./kit";

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
    <Page>
      <Section
        label="Detected shell"
        note="Auto-detected once at startup. Override only if detection guesses wrong."
      >
        <Card>
          <Row title="Binary">
            <span className="font-mono text-xs text-text-secondary">
              {status?.binary ?? "…"}
            </span>
          </Row>
          <Row title="Kind">
            <span className="text-xs text-text-secondary">
              {status?.kind ?? "…"}
            </span>
          </Row>
          <Row title="Version">
            <span className="font-mono text-xs text-text-secondary">
              {status?.version ?? "unknown"}
            </span>
          </Row>
          <Row title="Source">
            <span className="text-xs text-text-secondary">
              {status?.source ?? "…"}
            </span>
          </Row>
        </Card>
      </Section>

      <Section label="Override">
        <Card>
          <Row
            stacked
            title="Shell path"
            desc="Leave empty to fall back to whatever was detected."
          >
            <div className="flex gap-2">
              <input
                value={draft}
                onChange={(evt) => setDraft(evt.target.value)}
                onKeyDown={(evt) => {
                  if (evt.key === "Enter") void onSave();
                }}
                placeholder="/usr/bin/bash"
                spellCheck={false}
                className={INPUT + " font-mono text-xs"}
              />
              <Btn variant="primary" onClick={() => void onSave()}>
                Save
              </Btn>
              <Btn onClick={() => void onClear()}>Auto</Btn>
            </div>
            {err !== null && (
              <p className="text-xs text-red-400">{err}</p>
            )}
          </Row>
        </Card>
      </Section>
    </Page>
  );
}
