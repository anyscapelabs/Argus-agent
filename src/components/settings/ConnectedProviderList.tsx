import { useState } from "react";
import type { Provider } from "../../lib/ipc";

type ConnectedProviderRowProps = {
  provider: Provider;
  onDisconnect?: (id: string) => void;
  onConnect?: (id: string) => void;
  variant?: "connected" | "popular";
};

function ProviderLogo({ name, logoUrl }: { name: string; logoUrl: string | null }) {
  const [broken, setBroken] = useState(false);

  if (logoUrl && !broken) {
    return (
      <img
        src={logoUrl}
        alt=""
        onError={() => setBroken(true)}
        className="h-8 w-8 shrink-0 rounded-lg object-contain p-1 invert"
      />
    );
  }

  const initials = name
    .split(/[\s-]+/)
    .map((w) => w[0])
    .join("")
    .slice(0, 2)
    .toUpperCase();

  return (
    <div className="flex h-8 w-8 shrink-0 items-center justify-center rounded-lg bg-accent text-[11px] font-semibold leading-none text-bg-primary">
      {initials}
    </div>
  );
}

function ConnectedProviderRow({ provider, onDisconnect, onConnect, variant = "connected" }: ConnectedProviderRowProps) {
  const isPopular = variant === "popular";

  return (
    <div className="flex items-center justify-between gap-3 border-b border-border-primary px-2 py-1 last:border-b-0">
      <div className="flex min-w-0 items-center gap-3">
        <ProviderLogo name={provider.name} logoUrl={provider.logoUrl} />
        <span className="truncate text-sm font-medium text-text-primary">
          {provider.name}
        </span>
        {!isPopular && (
          <span className="inline-flex shrink-0 items-center rounded-full border border-border-primary bg-bg-hover-secondary px-2 py-0.5 text-[11px] font-medium leading-none text-text-secondary">
            Connected
          </span>
        )}
      </div>

      {isPopular ? (
        <button
          type="button"
          onClick={() => onConnect?.(provider.id)}
          className="shrink-0 rounded-full border border-border-primary bg-bg-hover-secondary px-4 py-1.5 text-sm font-medium text-text-primary transition-colors hover:bg-bg-hover-primary"
        >
          Connect
        </button>
      ) : (
        <button
          type="button"
          onClick={() => onDisconnect?.(provider.id)}
          className="shrink-0 rounded-full border border-border-primary bg-bg-hover-secondary px-4 py-1.5 text-sm font-medium text-text-primary transition-colors hover:bg-bg-hover-primary"
        >
          Disconnect
        </button>
      )}
    </div>
  );
}

type ConnectedProviderListProps = {
  providers: Provider[];
  onDisconnect?: (id: string) => void;
  onConnect?: (id: string) => void;
  variant?: "connected" | "popular";
};

export default function ConnectedProviderList({
  providers,
  onDisconnect,
  onConnect,
  variant = "connected",
}: ConnectedProviderListProps) {
  if (providers.length === 0) {
    return (
      <p className="text-sm text-text-secondary">
        {variant === "popular" ? "No popular providers." : "No connected providers."}
      </p>
    );
  }

  return (
    <div className="flex flex-col overflow-hidden rounded-xl border border-border-primary">
      {providers.map((p) => (
        <ConnectedProviderRow
          key={p.id}
          provider={p}
          onDisconnect={onDisconnect}
          onConnect={onConnect}
          variant={variant}
        />
      ))}
    </div>
  );
}
