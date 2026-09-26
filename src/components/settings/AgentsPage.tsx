import { useEffect, useState } from "react";

import { agentKeep, agentSetKeep } from "../../lib/ipc";
import { Card, Page, Row, Section, Segmented } from "./kit";

const KEEPS = [
  { value: 0, label: "All" },
  { value: 50, label: "50" },
  { value: 200, label: "200" },
  { value: 1000, label: "1000" },
];

const BLURBS: Record<number, string> = {
  0: "Keep every transcript. Disk grows with every sub-agent you start.",
  50: "The last 50 per chat. Older ones are deleted at the next start.",
  200: "The last 200 per chat. Older ones are deleted at the next start.",
  1000: "The last 1000 per chat. Older ones are deleted at the next start.",
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
    <Page>
      <Section label="Transcripts">
        <Card>
          <Row
            title="How many to keep"
            desc="The prune runs once, when Argus starts. A sub-agent that is still running is never one of the ones removed."
          >
            <Segmented
              value={String(keep)}
              opts={KEEPS.map((k) => ({ value: String(k.value), label: k.label }))}
              onChange={(v) => void pick(Number(v))}
            />
          </Row>
          <Row title="Currently" desc={BLURBS[keep]} />
        </Card>
      </Section>
    </Page>
  );
}
