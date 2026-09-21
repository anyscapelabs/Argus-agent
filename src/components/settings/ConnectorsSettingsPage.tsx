import { useEffect, useState } from "react";

import {
  connClearClient,
  connClearSecret,
  connHasClient,
  connHasToken,
  connRemoveToken,
  connSaveClient,
  connSaveSecret,
  connSaveToken,
} from "../../lib/ipc";
import {
  CONNECTOR_SERVICES,
  type ConnectorService,
  type FieldKind,
} from "../../lib/connectorCreds";

const SERVICES = CONNECTOR_SERVICES;

const EMPTY_VALS: Record<FieldKind, string> = {
  token: "",
  client: "",
  secret: "",
};

async function refreshSaved(id: string) {
  const [tok, cli] = await Promise.all([
    connHasToken(id).catch(() => false),
    connHasClient(id).catch(() => false),
  ]);

  return { token: tok, client: cli };
}

function ServiceRow({ svc }: { svc: ConnectorService }) {
  const [vals, setVals] = useState<Record<FieldKind, string>>(EMPTY_VALS);
  const [saved, setSaved] = useState({ token: false, client: false });
  const [busy, setBusy] = useState(false);
  const [note, setNote] = useState("");

  useEffect(() => {
    let alive = true;

    refreshSaved(svc.id).then((s) => {
      if (alive) setSaved(s);
    });

    return () => {
      alive = false;
    };
  }, [svc]);

  const save = async () => {
    if (busy) return;

    const filled = svc.fields.filter((f) => vals[f.kind].trim() !== "");
    if (filled.length === 0) return;

    setBusy(true);
    setNote("");

    try {
      for (const f of filled) {
        const v = vals[f.kind].trim();

        if (f.kind === "token") await connSaveToken(svc.id, v);
        else if (f.kind === "client") await connSaveClient(svc.id, v);
        else await connSaveSecret(svc.id, v);
      }

      setVals(EMPTY_VALS);
      setSaved(await refreshSaved(svc.id));
      setNote("Saved to keyring");
    } catch (err) {
      setNote(String(err));
    } finally {
      setBusy(false);
    }
  };

  const remove = async () => {
    if (busy) return;

    setBusy(true);
    setNote("");

    try {
      await connRemoveToken(svc.id);
      await connClearClient(svc.id);
      await connClearSecret(svc.id);
      setSaved({ token: false, client: false });
      setNote("Credentials removed");
    } catch (err) {
      setNote(String(err));
    } finally {
      setBusy(false);
    }
  };

  const hasSaved = saved.token || saved.client;
  const canSave = svc.fields.some((f) => vals[f.kind].trim() !== "");
  const status = hasSaved
    ? note || "Saved in the OS keyring — overrides built-in defaults"
    : note || "No credentials saved";

  return (
    <div className="rounded-xl px-2 py-2 transition-colors hover:bg-bg-hover-primary">
      <div className="flex items-center gap-3">
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2">
            <h3 className="text-sm font-medium text-text-primary">
              {svc.name}
            </h3>
            {hasSaved && (
              <span className="h-2 w-2 shrink-0 rounded-full bg-green-500" />
            )}
          </div>
          <p className="truncate text-xs text-text-secondary">{status}</p>
        </div>
        {hasSaved && (
          <button
            type="button"
            onClick={remove}
            disabled={busy}
            className={
              "shrink-0 rounded-full border border-border-primary px-2.5 " +
              "py-1 text-xs text-text-secondary hover:text-text-primary " +
              "cursor-pointer disabled:opacity-60"
            }
          >
            Remove
          </button>
        )}
      </div>
      <div className="mt-1.5 flex flex-wrap items-end gap-2">
        {svc.fields.map((f) => (
          <div key={f.kind} className="min-w-40 flex-1">
            <label className="text-xs text-text-secondary">{f.label}</label>
            <input
              type="password"
              value={vals[f.kind]}
              placeholder={f.placeholder}
              disabled={busy}
              autoComplete="off"
              onChange={(e) =>
                setVals((v) => ({ ...v, [f.kind]: e.target.value }))
              }
              className={
                "mt-0.5 w-full rounded-lg border border-border-primary " +
                "bg-bg-hover-secondary px-2 py-1 text-sm text-text-primary " +
                "placeholder:text-text-secondary focus:outline-none " +
                "focus:ring-1 focus:ring-text-secondary disabled:opacity-50"
              }
            />
          </div>
        ))}
        <button
          type="button"
          onClick={save}
          disabled={busy || !canSave}
          className={
            "shrink-0 rounded-lg bg-accent px-2.5 py-1 text-xs font-medium " +
            "text-bg-primary transition-opacity hover:opacity-90 " +
            "disabled:cursor-not-allowed disabled:opacity-50"
          }
        >
          {busy ? "Saving…" : "Save"}
        </button>
      </div>
    </div>
  );
}

export default function ConnectorsSettingsPage() {
  return (
    <div className="mx-auto w-full max-w-2xl py-2">
      <h1 className="text-lg font-medium text-text-primary">Connectors</h1>
      <p className="mt-0.5 text-xs text-text-secondary">
        Your own credentials, stored in the OS keyring. They override any
        built-in or environment defaults.
      </p>
      <div className="mt-3 flex flex-col gap-1">
        {SERVICES.map((svc) => (
          <ServiceRow key={svc.id} svc={svc} />
        ))}
      </div>
    </div>
  );
}
