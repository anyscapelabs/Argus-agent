import { useMemo, useState } from "react";
import { LuRefreshCw, LuSearch } from "react-icons/lu";

import { useProviders } from "../../hooks/useProviders";
import type { Provider } from "../../lib/ipc";
import ConnectedProviderList from "./ConnectedProviderList";
import { Btn, Card, Note, Page, Section } from "./kit";
import ProviderConnectModal from "./ProviderConnectModal";

// Past this many rows, scrolling beats a control nobody would find.
const SEARCH_OVER = 12;

// The catalog's own ranking, used to label rather than to hide.
function isPopular(p: Provider): boolean {
  return !p.connected && p.priority < 100;
}

export default function ProvidersPage() {
  const {
    providers,
    loading,
    syncing,
    err,
    connect,
    disconnect,
    syncCatalog,
  } = useProviders();
  const [selected, setSelected] = useState<Provider | null>(null);
  const [query, setQuery] = useState("");

  const groups = useMemo(() => {
    const q = query.trim().toLowerCase();

    const matching = providers.filter(
      (p) => q === "" || p.name.toLowerCase().includes(q),
    );

    const connected = matching.filter((p) => p.connected);
    const popular = matching.filter(isPopular);
    const rest = matching
      .filter((p) => !p.connected && !isPopular(p))
      .sort((a, b) => a.name.localeCompare(b.name));

    return [
      { label: "Connected", rows: connected },
      { label: "Popular", rows: popular },
      { label: "All providers", rows: rest },
    ].filter((g) => g.rows.length > 0);
  }, [providers, query]);

  const empty = groups.length === 0;

  function handleConnectClick(id: string): void {
    const provider = providers.find((x) => x.id === id);
    if (!provider) return;

    setSelected(provider);
  }

  async function handleModalConnect(
    provider: Provider,
    apiKey: string,
  ): Promise<void> {
    await connect(provider.id, apiKey);
    setSelected(null);
  }

  return (
    <Page>
      {providers.length > SEARCH_OVER && (
        <label className="relative -mt-4 block">
          <LuSearch
            size={14}
            className="pointer-events-none absolute top-1/2 left-3 -translate-y-1/2 text-text-tertiary"
          />
          <input
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="Search providers"
            className={
              "w-full rounded-lg border border-border-primary " +
              "bg-bg-secondary py-2 pr-3 pl-8 text-sm text-text-primary " +
              "outline-none placeholder:text-text-tertiary " +
              "focus:border-text-tertiary"
            }
          />
        </label>
      )}

      {loading ? (
        <Card>
          <div className="px-4 py-3.5">
            <Note>Loading providers…</Note>
          </div>
        </Card>
      ) : err !== null ? (
        <Card>
          <div className="px-4 py-3.5">
            <span className="text-xs text-red-400">{err}</span>
          </div>
        </Card>
      ) : empty ? (
        <Card>
          <div className="px-4 py-3.5">
            <Note>
              {providers.length === 0
                ? "No providers in the catalog yet. Sync to fetch them."
                : "Nothing matches that."}
            </Note>
          </div>
        </Card>
      ) : (
        groups.map((g) => (
          <Section key={g.label} label={g.label}>
            <Card>
              <ConnectedProviderList
                providers={g.rows}
                onConnect={handleConnectClick}
                onDisconnect={disconnect}
              />
            </Card>
          </Section>
        ))
      )}

      <div className="flex justify-end">
        <Btn onClick={() => void syncCatalog()} disabled={syncing}>
          <LuRefreshCw size={12} className={syncing ? "animate-spin" : undefined} />
          {syncing ? "Syncing…" : "Sync catalog"}
        </Btn>
      </div>

      <ProviderConnectModal
        open={selected !== null}
        provider={selected}
        onClose={() => setSelected(null)}
        onConnect={handleModalConnect}
      />
    </Page>
  );
}
