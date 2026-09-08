import { useEffect, useState } from "react";
import { LuArrowLeft, LuX } from "react-icons/lu";
import type { Provider } from "../../lib/ipc";

type ProviderConnectModalProps = {
  open: boolean;
  provider: Provider | null;
  onClose: () => void;
  onConnect: (provider: Provider, apiKey: string) => void;
};

export default function ProviderConnectModal({ open, provider, onClose, onConnect }: ProviderConnectModalProps) {
  const [apiKey, setApiKey] = useState("");

  useEffect(() => {
    if (open) {
      setApiKey("");
      const onKey = (e: KeyboardEvent) => {
        if (e.key === "Escape") onClose();
      };
      document.addEventListener("keydown", onKey);
      return () => document.removeEventListener("keydown", onKey);
    }
  }, [open, onClose]);

  if (!open || !provider) return null;

  const handleConnect = () => {
    if (!apiKey.trim()) return;
    onConnect(provider, apiKey.trim());
  };

  const initials = provider.name
    .split(/[\s-]+/)
    .map((w) => w[0])
    .join("")
    .slice(0, 1)
    .toUpperCase();

  const logo = provider.logoUrl && (
    <img
      src={provider.logoUrl}
      alt=""
      className="h-6 w-6 shrink-0 rounded object-contain p-0.5 invert"
    />
  );

  return (
    <div
      className="fixed inset-0 z-[60] flex items-center justify-center bg-black/60 p-4"
      onClick={onClose}
      role="dialog"
      aria-modal="true"
      aria-label={`Connect ${provider.name}`}
    >
      <div
        className="flex w-full max-w-[560px] min-h-[520px] flex-col rounded-xl border border-border-primary bg-bg-secondary p-6 shadow-4xl"
        onClick={(e) => e.stopPropagation()}
      >
        {/* Header: back + close */}
        <div className="flex items-center justify-between">
          <button
            type="button"
            onClick={onClose}
            aria-label="Back"
            className="flex h-6 w-6 items-center justify-center rounded text-text-secondary transition-colors hover:bg-bg-hover-secondary hover:text-text-primary"
          >
            <LuArrowLeft size={14} />
          </button>
          <button
            type="button"
            onClick={onClose}
            aria-label="Close"
            className="flex h-6 w-6 items-center justify-center rounded text-text-secondary transition-colors hover:bg-bg-hover-secondary hover:text-text-primary"
          >
            <LuX size={14} />
          </button>
        </div>

        {/* Title with logo */}
        <div className="mt-3 flex items-center gap-2">
          {logo ?? (
            <div className="flex h-6 w-6 shrink-0 items-center justify-center rounded bg-accent text-[11px] font-bold leading-none text-bg-primary">
              {initials}
            </div>
          )}
          <h3 className="text-sm font-medium text-text-primary">Connect {provider.name}</h3>
        </div>

        <p className="mt-3 text-sm leading-relaxed text-text-secondary">
          {`Enter your ${provider.name} API key to connect your account and use ${provider.name} models in Argus.`}
        </p>

        <div className="mt-4 flex flex-col gap-1.5">
          <label htmlFor="provider-api-key" className="text-sm text-text-secondary">
            {provider.name} API key
          </label>
          <input
            id="provider-api-key"
            type="password"
            value={apiKey}
            onChange={(e) => setApiKey(e.target.value)}
            placeholder="API key"
            className="w-full rounded-lg border border-border-primary bg-bg-hover-secondary px-2 py-1.5 text-sm text-text-primary placeholder:text-text-secondary focus:border-text-secondary focus:outline-none focus:ring-1 focus:ring-text-secondary"
            autoFocus
          />
        </div>

        <div className="mt-auto flex justify-end pt-6">
          <button
            type="button"
            onClick={handleConnect}
            disabled={!apiKey.trim()}
            className="rounded-lg bg-accent px-2 py-1 text-sm font-medium text-bg-primary transition-opacity hover:opacity-90 disabled:cursor-not-allowed disabled:opacity-50"
          >
            Continue
          </button>
        </div>
      </div>
    </div>
  );
}
