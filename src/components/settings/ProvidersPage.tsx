import { useMemo, useState } from "react";
import { LuRefreshCw, LuSearch } from "react-icons/lu";

import { useProviders } from "../../hooks/useProviders";
import type { Provider } from "../../lib/ipc";
import ConnectedProviderList from "./ConnectedProviderList";
import { Btn, Card, Note, Page, Section } from "./kit";
import ProviderConnectModal from "./ProviderConnectModal";

/// Enough rows that scrolling beats a control nobody would find. Below it the
/// box is just furniture.
const SEARCH_OVER = 12;

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

  // One list, connected first, everything in it. Splitting this into
  // "connected" and "popular" behind a View more button is what made it read
  // as though the eight shown were the eight that exist.
  const shown = useMemo(() => {
    const q = query.trim().toLowerCase();

    return providers
      .filter((p) => q === "" || p.name.toLowerCase().includes(q))
      .slice()
      .sort((a, b) => {
        if (a.connected !== b.connected) {
          return a.connected ? -1 : 1;
        }

        return a.priority - b.priority;
      });
  }, [providers, query]);

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
      <Section
        label="Providers"
        note="Your key goes to the OS keyring and never leaves this machine."
      >
        <div className="flex flex-col gap-3">
          {providers.length > SEARCH_OVER && (
            <label className="relative block">
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

          <Card>
            {loading ? (
              <div className="px-4 py-3.5">
                <Note>Loading providers…</Note>
              </div>
            ) : err !== null ? (
              <div className="px-4 py-3.5">
                <span className="text-xs text-red-400">{err}</span>
              </div>
            ) : shown.length === 0 ? (
              <div className="px-4 py-3.5">
                <Note>
                  {providers.length === 0
                    ? "No providers in the catalog yet. Sync to fetch them."
                    : "Nothing matches that."}
                </Note>
              </div>
            ) : (
              <ConnectedProviderList
                providers={shown}
                onConnect={handleConnectClick}
                onDisconnect={disconnect}
              />
            )}
          </Card>
        </div>
      </Section>

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
