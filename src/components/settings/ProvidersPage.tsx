import { useState } from "react";
import { LuRefreshCw } from "react-icons/lu";

import { useProviders } from "../../hooks/useProviders";
import type { Provider } from "../../lib/ipc";
import ConnectedProviderList from "./ConnectedProviderList";
import { Btn, Card, Note, Page, Section } from "./kit";
import ProviderConnectModal from "./ProviderConnectModal";

function isPopular(provider: Provider): boolean {
  return !provider.connected && provider.priority < 100;
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
  const [showAll, setShowAll] = useState(false);

  const connected = providers.filter((p) => p.connected);
  const popular = showAll
    ? providers.filter((p) => !p.connected)
    : providers.filter(isPopular);
  const hasMore = providers.some(
    (p) => !p.connected && p.priority >= 100,
  );

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
        label="Connected"
        note="Providers with a key in your keyring. The key never leaves the machine."
      >
        <Card>
          {loading ? (
            <div className="px-4 py-3.5">
              <Note>Loading providers…</Note>
            </div>
          ) : err !== null ? (
            <div className="px-4 py-3.5">
              <span className="text-xs text-red-400">{err}</span>
            </div>
          ) : (
            <ConnectedProviderList
              providers={connected}
              onDisconnect={disconnect}
              variant="connected"
            />
          )}
        </Card>
      </Section>

      {!loading && err === null && (
        <Section label="Add a provider">
          <Card>
            <ConnectedProviderList
              providers={popular}
              onConnect={handleConnectClick}
              variant="popular"
            />
            {hasMore && (
              <div className="px-4 py-3">
                <Btn variant="ghost" onClick={() => setShowAll((v) => !v)}>
                  {showAll ? "View less" : "View more"}
                </Btn>
              </div>
            )}
          </Card>
        </Section>
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
