import { useState } from "react";
import { LuRefreshCw } from "react-icons/lu";

import { useProviders } from "../../hooks/useProviders";
import type { Provider } from "../../lib/ipc";
import ConnectedProviderList from "./ConnectedProviderList";
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
    <div className="flex flex-col gap-6">
      <div className="flex flex-col gap-1">
        <h2 className="text-sm font-semibold text-text-primary">
          Providers
        </h2>
        <p className="text-sm text-text-secondary">
          Manage your model providers and configure API keys.
        </p>
      </div>

      <section className="flex flex-col gap-3">
        <div className="flex items-center justify-between">
          <h3 className="text-sm font-medium text-text-primary">
            Connected Providers
          </h3>
          <button
            type="button"
            onClick={syncCatalog}
            disabled={syncing}
            title="Update provider and model catalog from models.dev"
            className="flex items-center gap-1.5 rounded-full border border-border-primary bg-bg-hover-secondary px-3 py-1 text-xs font-medium text-text-secondary transition-colors hover:bg-bg-hover-primary hover:text-text-primary disabled:opacity-50"
          >
            <LuRefreshCw
              size={12}
              className={syncing ? "animate-spin" : undefined}
            />
            {syncing ? "Syncing…" : "Sync catalog"}
          </button>
        </div>

        {loading ? (
          <p className="text-sm text-text-secondary">
            Loading providers…
          </p>
        ) : err ? (
          <p className="text-sm text-red-400">{err}</p>
        ) : (
          <ConnectedProviderList
            providers={connected}
            onDisconnect={disconnect}
            variant="connected"
          />
        )}
      </section>

      {!loading && !err && (
        <section className="flex flex-col gap-3">
          <h3 className="text-sm font-medium text-text-primary">
            Popular Providers
          </h3>
          <ConnectedProviderList
            providers={popular}
            onConnect={handleConnectClick}
            variant="popular"
          />
          {hasMore && (
            <button
              type="button"
              onClick={() => setShowAll((v) => !v)}
              className="self-center rounded-full px-3 py-1 text-xs font-medium text-text-secondary transition-colors hover:bg-bg-hover-secondary hover:text-text-primary"
            >
              {showAll ? "View less" : "View more"}
            </button>
          )}
        </section>
      )}

      <ProviderConnectModal
        open={selected !== null}
        provider={selected}
        onClose={() => setSelected(null)}
        onConnect={handleModalConnect}
      />
    </div>
  );
}
