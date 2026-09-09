import { useEffect, useState } from "react";
import { LuArrowLeft, LuX } from "react-icons/lu";

import { useProviderLogo } from "../../hooks/useProviderLogo";
import type { Provider } from "../../lib/ipc";

type ProviderConnectModalProps = {
  open: boolean;
  provider: Provider | null;
  onClose: () => void;
  onConnect: (provider: Provider, apiKey: string) => Promise<void>;
};

export default function ProviderConnectModal({
  open,
  provider,
  onClose,
  onConnect,
}: ProviderConnectModalProps) {
  const [apiKey, setApiKey] = useState("");
  const [saving, setSaving] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  const logoUri = useProviderLogo(provider?.id ?? "");

  useEffect(() => {
    if (!open) return;

    setApiKey("");
    setErr(null);
    setSaving(false);

    function onKey(event: KeyboardEvent): void {
      if (event.key === "Escape") onClose();
    }

    document.addEventListener("keydown", onKey);

    return () => document.removeEventListener("keydown", onKey);
  }, [open, onClose]);

  if (!open || !provider) return null;

  async function handleConnect(): Promise<void> {
    if (!provider) return;

    const key = apiKey.trim();
    if (!key || saving) return;

    setSaving(true);
    setErr(null);

    try {
      await onConnect(provider, key);
    } catch (e) {
      setErr(String(e));
      setSaving(false);
      return;
    }

    setApiKey("");
    setSaving(false);
    onClose();
  }

  const initials = provider.name
    .split(/[\s-]+/)
    .map((w) => w[0])
    .join("")
    .slice(0, 1)
    .toUpperCase();

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

        <div className="mt-3 flex items-center gap-2">
          {logoUri ? (
            <img
              src={logoUri}
              alt=""
              className="h-6 w-6 shrink-0 rounded object-contain p-0.5 invert"
            />
          ) : (
            <div className="flex h-6 w-6 shrink-0 items-center justify-center rounded bg-accent text-[11px] font-bold leading-none text-bg-primary">
              {initials}
            </div>
          )}
          <h3 className="text-sm font-medium text-text-primary">
            Connect {provider.name}
          </h3>
        </div>

        <p className="mt-3 text-sm leading-relaxed text-text-secondary">
          {`Enter your ${provider.name} API key to connect your account and use ${provider.name} models in Argus.`}
        </p>

        <div className="mt-4 flex flex-col gap-1.5">
          <label
            htmlFor="provider-api-key"
            className="text-sm text-text-secondary"
          >
            {provider.name} API key
          </label>
          <input
            id="provider-api-key"
            type="password"
            value={apiKey}
            onChange={(e) => setApiKey(e.target.value)}
            placeholder="API key"
            disabled={saving}
            className="w-full rounded-lg border border-border-primary bg-bg-hover-secondary px-2 py-1.5 text-sm text-text-primary placeholder:text-text-secondary focus:border-text-secondary focus:outline-none focus:ring-1 focus:ring-text-secondary disabled:opacity-50"
            autoFocus
          />
          {err && <p className="text-xs text-red-400">{err}</p>}
        </div>

        <div className="mt-auto flex justify-end pt-6">
          <button
            type="button"
            onClick={handleConnect}
            disabled={!apiKey.trim() || saving}
            className="rounded-lg bg-accent px-2 py-1 text-sm font-medium text-bg-primary transition-opacity hover:opacity-90 disabled:cursor-not-allowed disabled:opacity-50"
          >
            {saving ? "Verifying…" : "Continue"}
          </button>
        </div>
      </div>
    </div>
  );
}
