import { useEffect, useState } from "react";
import { LuX } from "react-icons/lu";

import ConnectorIcon from "./ConnectorIcon";
import { connSaveClient, connSaveSecret, connSaveToken } from "../lib/ipc";
import type { ConnectorService } from "../lib/connectorCreds";

type Props = {
  open: boolean;
  service: ConnectorService | null;
  onClose: () => void;
  onSaved: (service: ConnectorService) => void;
};

const EMPTY: Record<string, string> = { token: "", client: "", secret: "" };

export default function ConnectorConnectModal({
  open,
  service,
  onClose,
  onSaved,
}: Props) {
  const [vals, setVals] = useState<Record<string, string>>(EMPTY);
  const [saving, setSaving] = useState(false);
  const [err, setErr] = useState<string | null>(null);

  useEffect(() => {
    if (!open) return;

    setVals(EMPTY);
    setErr(null);
    setSaving(false);

    function onKey(event: KeyboardEvent): void {
      if (event.key === "Escape") onClose();
    }

    document.addEventListener("keydown", onKey);

    return () => document.removeEventListener("keydown", onKey);
  }, [open, onClose]);

  if (!open || !service) return null;

  const filled = service.fields.some((f) => (vals[f.kind] ?? "").trim() !== "");

  async function save(): Promise<void> {
    if (!service || saving || !filled) return;

    setSaving(true);
    setErr(null);

    try {
      for (const f of service.fields) {
        const v = (vals[f.kind] ?? "").trim();
        if (v === "") continue;

        if (f.kind === "token") await connSaveToken(service.id, v);
        else if (f.kind === "client") await connSaveClient(service.id, v);
        else await connSaveSecret(service.id, v);
      }

      onSaved(service);
      onClose();
    } catch (e) {
      setErr(String(e));
      setSaving(false);
    }
  }

  return (
    <div
      className="fixed inset-0 z-[60] flex items-center justify-center bg-black/60 p-4"
      onClick={onClose}
      role="dialog"
      aria-modal="true"
      aria-label={`Connect ${service.name}`}
    >
      <div
        className="flex w-full max-w-[480px] flex-col rounded-xl border border-border-primary bg-bg-secondary p-6 shadow-4xl"
        onClick={(evt) => evt.stopPropagation()}
      >
        <div className="flex items-center justify-end">
          <button
            type="button"
            onClick={onClose}
            aria-label="Close"
            className="flex h-6 w-6 items-center justify-center rounded text-text-secondary transition-colors hover:bg-bg-hover-secondary hover:text-text-primary"
          >
            <LuX size={14} />
          </button>
        </div>

        <div className="mt-1 flex items-center gap-2.5">
          <ConnectorIcon id={service.id} size={22} />
          <h3 className="text-sm font-medium text-text-primary">
            Connect {service.name}
          </h3>
        </div>

        <p className="mt-3 text-sm leading-relaxed text-text-secondary">
          {`Paste your ${service.name} credentials — they are stored in the OS keyring and never leave this machine.`}
        </p>

        <div className="mt-4 flex flex-col gap-3">
          {service.fields.map((f) => (
            <div key={f.kind} className="flex flex-col gap-1.5">
              <label htmlFor={`conn-${service.id}-${f.kind}`} className="text-sm text-text-secondary">
                {f.label}
              </label>
              <input
                id={`conn-${service.id}-${f.kind}`}
                type="password"
                value={vals[f.kind] ?? ""}
                placeholder={f.placeholder}
                disabled={saving}
                autoComplete="off"
                onChange={(e) =>
                  setVals((v) => ({ ...v, [f.kind]: e.target.value }))
                }
                className="w-full rounded-lg border border-border-primary bg-bg-hover-secondary px-2 py-1.5 text-sm text-text-primary placeholder:text-text-secondary focus:border-text-secondary focus:outline-none focus:ring-1 focus:ring-text-secondary disabled:opacity-50"
                autoFocus={service.fields[0].kind === f.kind}
              />
            </div>
          ))}
          {err && <p className="text-xs text-red-400">{err}</p>}
        </div>

        <div className="mt-6 flex justify-end">
          <button
            type="button"
            onClick={save}
            disabled={!filled || saving}
            className="rounded-lg bg-accent px-3 py-1.5 text-sm font-medium text-bg-primary transition-opacity hover:opacity-90 disabled:cursor-not-allowed disabled:opacity-50"
          >
            {saving ? "Saving…" : "Save"}
          </button>
        </div>
      </div>
    </div>
  );
}
