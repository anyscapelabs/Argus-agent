import { useEffect, useRef, useState } from "react";
import { LuChevronDown, LuX } from "react-icons/lu";

import Dropdown from "./Dropdown";

import {
  connClearClient,
  connClearSecret,
  connHasClient,
  connHasToken,
  connRemoveToken,
  connectorTools,
  type CatalogTool,
} from "../lib/ipc";
import { useOAuthFlow, type OAuthSvc } from "../lib/oauthFlow";
import type { ConnectorService } from "../lib/connectorCreds";
import ConnectorIcon from "./ConnectorIcon";
import { oauthSvcById } from "./ConnectorOAuthCard";

type Props = {
  open: boolean;
  svc: ConnectorService | null;
  onClose: () => void;
  onConnectKeys: (svc: ConnectorService) => void;
  onOwnApp: (svc: ConnectorService) => void;
  onDisabled: () => void;
};

const NOOP: OAuthSvc = {
  id: "",
  name: "",
  tagline: "",
  flow: "loopback",
  status: async () => ({ connected: false }),
  begin: async () => ({}),
  disconnect: async () => {},
};

export default function ConnectorDetailModal({
  open,
  svc,
  onClose,
  onConnectKeys,
  onOwnApp,
  onDisabled,
}: Props) {
  const oauth = svc ? oauthSvcById(svc.id) : undefined;
  const flow = useOAuthFlow(oauth ?? NOOP);
  const [saved, setSaved] = useState({ token: false, client: false });
  const [tools, setTools] = useState<CatalogTool[]>([]);
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  const alive = useRef(true);

  useEffect(() => {
    if (!open || !svc) return;

    alive.current = true;
    setErr(null);
    setSaved({ token: false, client: false });

    Promise.all([
      connHasToken(svc.id).catch(() => false),
      connHasClient(svc.id).catch(() => false),
      connectorTools().catch(() => []),
    ]).then(([tok, cli, cat]) => {
      if (!alive.current) return;

      setSaved({ token: tok, client: cli });
      setTools(cat.filter((t) => t.name.startsWith(`${svc.id}.`)));
    });

    return () => {
      alive.current = false;
    };
  }, [open, svc]);

  if (!open || !svc) return null;

  const connected = oauth
    ? flow.state === "connected"
    : saved.token || saved.client;
  const isOauth = oauth !== undefined;

  const disable = async () => {
    setBusy(true);
    setErr(null);

    try {
      if (oauth) await oauth.disconnect();
      await connRemoveToken(svc.id);
      await connClearClient(svc.id);
      await connClearSecret(svc.id);
      onDisabled();
      onClose();
    } catch (e) {
      setErr(String(e));
    } finally {
      setBusy(false);
    }
  };

  const mainAction = () => {
    if (connected || busy) return;

    if (isOauth) void flow.connect();
    else onConnectKeys(svc);
  };

  const mainLabel =
    connected ? "Connected" : isOauth ? "Connect" : "Connect API keys";

  return (
    <div
      className="fixed inset-0 z-[55] flex items-center justify-center bg-black/60 p-4"
      onClick={onClose}
      role="dialog"
      aria-modal="true"
      aria-label={`${svc.name} details`}
    >
      <div
        className="relative flex max-h-[90vh] w-full max-w-3xl flex-col overflow-hidden rounded-2xl border border-border-primary bg-bg-secondary shadow-4xl"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-start justify-between gap-4 px-7 pt-6">
          <div className="flex min-w-0 items-center gap-4">
            <div
              className={
                "flex h-14 w-14 shrink-0 items-center justify-center " +
                "rounded-xl border border-border-primary bg-bg-primary"
              }
            >
              <ConnectorIcon id={svc.id} size={30} />
            </div>
            <div className="min-w-0">
              <h2 className="truncate text-lg font-medium text-text-primary">
                {svc.name}
              </h2>
              <p className="truncate text-sm text-text-secondary">
                {svc.tagline}
              </p>
            </div>
          </div>
          <div className="flex shrink-0 items-center gap-2">
            <div className="flex">
              <button
                type="button"
                onClick={mainAction}
                disabled={connected || busy}
                className={
                  "rounded-l-full px-3.5 py-1.5 text-xs font-medium " +
                  "transition-colors focus:outline-none " +
                  `${
                    connected
                      ? "bg-green-600 text-white cursor-default"
                      : "bg-accent text-bg-primary hover:opacity-90 cursor-pointer"
                  }`
                }
              >
                {busy ? "Working…" : mainLabel}
              </button>
              <Dropdown
                align="right"
                side="bottom"
                trigger={({ open, toggle }) => (
                  <button
                    type="button"
                    onClick={toggle}
                    aria-label="More actions"
                    className={
                      "rounded-r-full px-2 py-1.5 focus:outline-none " +
                      `${
                        connected
                          ? "bg-green-600 text-white"
                          : "bg-accent text-bg-primary"
                      } cursor-pointer`
                    }
                  >
                    <LuChevronDown
                      size={13}
                      className={`transition-transform ${open ? "rotate-180" : ""}`}
                    />
                  </button>
                )}
                items={[
                  ...(isOauth
                    ? [
                        {
                          label: "Use my own OAuth app",
                          onClick: () => onOwnApp(svc),
                        },
                      ]
                    : []),
                  ...(connected
                    ? [
                        {
                          label: isOauth
                            ? "Disconnect & remove keys"
                            : "Disable & remove keys",
                          danger: true,
                          disabled: busy,
                          onClick: () => {
                            void disable();
                          },
                        },
                      ]
                    : []),
                ]}
              />
            </div>
            <button
              type="button"
              onClick={onClose}
              aria-label="Close"
              className="flex h-6 w-6 items-center justify-center rounded text-text-secondary transition-colors hover:bg-bg-hover-secondary hover:text-text-primary"
            >
              <LuX size={14} />
            </button>
          </div>
        </div>

        <div className="min-h-0 flex-1 overflow-y-auto px-7 pb-7 pt-4">
          <div className="flex gap-8">
            <div className="min-w-0 flex-1">
              <p className="text-sm leading-relaxed text-text-secondary">
                {svc.details ?? svc.tagline}
              </p>

              <h3 className="mt-5 text-sm font-medium text-text-primary">
                Tools
              </h3>
              {tools.length > 0 ? (
                <div className="mt-2 flex flex-wrap gap-1.5">
                  {tools.map((t) => (
                    <span
                      key={t.name}
                      title={t.desc}
                      className={
                        "rounded-md border border-border-primary " +
                        "bg-bg-hover-secondary px-2 py-1 font-mono text-xs " +
                        "text-text-primary"
                      }
                    >
                      {t.name}
                      {t.mutating && (
                        <span
                          className="ml-1.5 text-[10px] text-text-secondary"
                          title="writes — pauses for approval in ask mode"
                        >
                          ✎
                        </span>
                      )}
                    </span>
                  ))}
                </div>
              ) : (
                <p className="mt-2 text-xs text-text-secondary">
                  Loading tools…
                </p>
              )}

              <div
                className={
                  "mt-5 flex items-start gap-2 rounded-lg border " +
                  "border-border-primary bg-bg-primary px-3 py-2.5"
                }
              >
                <p className="text-xs leading-relaxed text-text-secondary">
                  The agent can only call the tools listed above. Write tools
                  pause for your approval in ask mode, and every call is logged
                  with secrets redacted.
                </p>
              </div>

              {err && <p className="mt-3 text-xs text-red-400">{err}</p>}
            </div>

            <div className="w-44 shrink-0">
              <p className="text-xs font-medium uppercase tracking-wide text-text-secondary">
                Sign-in
              </p>
              <p className="mt-0.5 text-xs text-text-primary">
                {isOauth ? "Required (browser)" : "Required (API key)"}
              </p>

              <p className="mt-4 text-xs font-medium uppercase tracking-wide text-text-secondary">
                Credentials
              </p>
              <p className="mt-0.5 text-xs text-text-primary">OS keyring</p>

              {isOauth && connected && flow.who && (
                <>
                  <p className="mt-4 text-xs font-medium uppercase tracking-wide text-text-secondary">
                    Account
                  </p>
                  <p className="mt-0.5 truncate text-xs text-text-primary">
                    {flow.who}
                  </p>
                </>
              )}

              {!isOauth && (
                <>
                  <p className="mt-4 text-xs font-medium uppercase tracking-wide text-text-secondary">
                    Saved
                  </p>
                  <p className="mt-0.5 text-xs text-text-primary">
                    {saved.token && saved.client
                      ? "API key + token"
                      : saved.token
                        ? "Token"
                        : saved.client
                          ? "Client ID"
                          : "Nothing yet"}
                  </p>
                </>
              )}

              <p className="mt-4 text-xs font-medium uppercase tracking-wide text-text-secondary">
                Tools
              </p>
              <p className="mt-0.5 text-xs text-text-primary">
                {tools.length} available
              </p>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
