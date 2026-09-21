import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";
import { LuLoaderCircle } from "react-icons/lu";

import {
  connHasClient,
  connHasToken,
  sessBrowserImport,
  sessExtInstall,
  sessExtStatus,
  sessExtUninstall,
} from "../lib/ipc";
import { CONNECTOR_SERVICES, type ConnectorService } from "../lib/connectorCreds";
import ConnectorIcon from "./ConnectorIcon";
import ConnectorConnectModal from "./ConnectorConnectModal";
import ConnectorOAuthCard, { OAUTH_SERVICES } from "./ConnectorOAuthCard";
import ConnectorDetailPage from "./ConnectorDetailPage";

const GRID_IDS = [
  "linear",
  "slack",
  "notion",
  "figma",
  "discord",
  "telegram",
  "todoist",
  "gitlab",
  "ha",
  "trello",
];

const GRID: ConnectorService[] = CONNECTOR_SERVICES.filter((s) =>
  GRID_IDS.includes(s.id),
);

type ExtState = "off" | "busy" | "granted" | "connected";
type ImportState = "idle" | "busy" | "ok" | "err";

const ON_KEY = "argus.ext.enabled";

type GridProps = {
  onOpen: (svc: ConnectorService) => void;
  onConnectKeys: (svc: ConnectorService) => void;
};

function ConnectorGrid({ onOpen, onConnectKeys }: GridProps) {
  const [connected, setConnected] = useState<Record<string, boolean>>({});

  const refresh = async () => {
    const entries = await Promise.all(
      GRID.map(async (s) => {
        const tok = await connHasToken(s.id).catch(() => false);
        const cli = await connHasClient(s.id).catch(() => false);

        return [s.id, tok || cli] as const;
      }),
    );

    setConnected(Object.fromEntries(entries));
  };

  useEffect(() => {
    void refresh();
  }, []);

  return (
    <div className="mt-2 grid grid-cols-2 gap-4">
      {GRID.map((svc) => {
        const on = connected[svc.id] ?? false;

        return (
          <div
            key={svc.id}
            onClick={() => onOpen(svc)}
            className="flex cursor-pointer items-center gap-3 rounded-xl bg-transparent px-2 py-1 transition-colors hover:bg-bg-hover-primary"
          >
            <div
              className={
                "flex h-11 w-11 shrink-0 items-center justify-center " +
                "rounded-lg border border-border-primary bg-bg-primary"
              }
            >
              <ConnectorIcon id={svc.id} size={20} />
            </div>
            <div className="min-w-0 flex-1">
              <h3 className="truncate text-sm font-medium text-text-primary">
                {svc.name}
              </h3>
              <p className="truncate text-xs text-text-secondary">
                {svc.tagline}
              </p>
            </div>
            {on ? (
              <span
                onClick={(e) => {
                  e.stopPropagation();
                  onOpen(svc);
                }}
                className={
                  "shrink-0 rounded-md bg-green-600 px-3 py-1 text-xs " +
                  "font-medium text-white cursor-pointer"
                }
              >
                Connected
              </span>
            ) : (
              <button
                type="button"
                onClick={(e) => {
                  e.stopPropagation();
                  onConnectKeys(svc);
                }}
                className={
                  "shrink-0 rounded-md border border-border-primary " +
                  "bg-bg-hover-secondary px-3 py-1 text-xs font-medium " +
                  "text-text-primary transition-colors " +
                  "hover:bg-bg-hover-primary cursor-pointer"
                }
              >
                Connect
              </button>
            )}
          </div>
        );
      })}
      {OAUTH_SERVICES.map((svc) => (
        <ConnectorOAuthCard
          key={svc.id}
          svc={svc}
          onOpen={(id) => {
            const s = CONNECTOR_SERVICES.find((x) => x.id === id);

            if (s) onOpen(s);
          }}
        />
      ))}
    </div>
  );
}

function BrowserCard() {
  const [extState, setExtState] = useState<ExtState>(() =>
    localStorage.getItem(ON_KEY) === "on" ? "granted" : "off"
  );
  const [note, setNote] = useState("");
  const [impState, setImpState] = useState<ImportState>("idle");
  const [impNote, setImpNote] = useState("");

  useEffect(() => {
    if (extState === "off" || extState === "busy") return;

    const poll = setInterval(async () => {
      try {
        const up = await sessExtStatus();

        if (up) {
          setExtState("connected");
          setNote("");
        }
      } catch {
      }
    }, 2_000);

    return () => clearInterval(poll);
  }, [extState]);

  useEffect(() => {
    const unDone = listen("browser-import-done", (e) => {
      const p = e.payload as { Ok?: string; Err?: string } | string | null;

      if (typeof p === "string") {
        setImpState("ok");
        setImpNote(p);
        return;
      }

      if (p && p.Err) {
        setImpState("err");
        setImpNote(p.Err);
        return;
      }

      setImpState("ok");
      setImpNote(p?.Ok ?? "Profile imported");
    });

    const unProgress = listen<number>("browser-import-progress", (e) => {
      setImpState("busy");
      setImpNote(`Importing… ${Math.round(e.payload / 1_048_576)} MB copied`);
    });

    return () => {
      void unDone.then((f) => f());
      void unProgress.then((f) => f());
    };
  }, []);

  const enable = async () => {
    setExtState("busy");
    setNote("Saving permission…");

    try {
      await sessExtInstall();
      localStorage.setItem(ON_KEY, "on");
      setExtState("granted");
      setNote("Permission saved — Chrome will open the first time the agent needs it.");
    } catch (err) {
      setExtState("off");
      setNote(String(err));
      localStorage.removeItem(ON_KEY);
    }
  };

  const runSetup = async () => {
    setNote("Opening chrome://extensions…");

    try {
      const res = await sessExtInstall();
      await openUrl("chrome://extensions").catch(() => {});
      await import("@tauri-apps/plugin-opener").then((m) =>
        m.openPath(res.extPath).catch(() => {})
      );
      setNote("Click “Load unpacked” and pick the revealed Argus extension folder.");
    } catch (err) {
      setNote(String(err));
    }
  };

  const disable = async () => {
    setExtState("off");
    setNote("");
    localStorage.removeItem(ON_KEY);

    try {
      await sessExtUninstall();
    } catch {
    }
  };

  const toggle = () => {
    if (extState === "busy") return;
    if (extState === "off") void enable();
    else void disable();
  };

  const runImport = async () => {
    setImpState("busy");
    setImpNote("Preparing import…");

    try {
      await sessBrowserImport("main");
    } catch (err) {
      setImpState("err");
      setImpNote(String(err));
    }
  };

  const extOn = extState === "granted" || extState === "connected";

  const statusLine =
    extState === "connected"
      ? "Connected — the agent can act in your Chrome."
      : extState === "granted"
        ? note
        : extState === "busy"
          ? "Saving permission…"
          : "Let the agent act inside your Chrome. Chrome only opens when it needs to.";

  return (
    <div className="rounded-xl bg-transparent px-2 py-1 transition-colors hover:bg-bg-hover-primary">
      <div className="flex items-center gap-4">
        <div
          className={
            "flex h-12 w-12 shrink-0 items-center justify-center rounded-lg " +
            "border border-border-primary bg-bg-primary text-xl " +
            "text-text-primary"
          }
        >
          <ConnectorIcon id="chrome" size={24} />
        </div>
        <div className="flex-1 min-w-0">
          <div className="flex items-center gap-2">
            <h3 className="truncate text-sm font-medium text-text-primary">
              Google Chrome
            </h3>
            {extState === "connected" && (
              <span className="h-2 w-2 shrink-0 rounded-full bg-green-500" />
            )}
          </div>
          <p className="truncate text-xs text-text-secondary">{statusLine}</p>
        </div>
        <button
          type="button"
          onClick={toggle}
          className={
            "relative h-6 w-11 shrink-0 rounded-full transition-colors cursor-pointer " +
            `${extOn ? "bg-green-600" : "bg-bg-hover-primary border border-border-primary"}`
          }
          aria-pressed={extOn}
          aria-label="Allow the agent to use Google Chrome"
        >
          <span
            className={
              "absolute top-0.5 h-5 w-5 rounded-full bg-white transition-all " +
              `${extOn ? "left-[22px]" : "left-0.5"}`
            }
          />
        </button>
      </div>
      {extOn && extState !== "connected" && (
        <div className="ml-16 mt-1.5">
          <button
            type="button"
            onClick={runSetup}
            className={
              "flex items-center gap-1.5 rounded-md border " +
              "border-border-primary px-2.5 py-1 text-xs text-text-secondary " +
              "hover:text-text-primary cursor-pointer"
            }
          >
            Set up extension
          </button>
        </div>
      )}
      {extOn && (
        <div className="ml-16 mt-1.5 flex items-center gap-3">
          <button
            type="button"
            onClick={runImport}
            disabled={impState === "busy"}
            className={
              "flex items-center gap-1.5 rounded-md border " +
              "border-border-primary px-2.5 py-1 text-xs text-text-secondary " +
              "hover:text-text-primary cursor-pointer disabled:opacity-60"
            }
          >
            {impState === "busy" && (
              <LuLoaderCircle size={11} className="animate-spin" />
            )}
            Import my Chrome profile (isolated mode)
          </button>
          {impNote && (
            <span className="truncate text-xs text-text-secondary">
              {impNote}
            </span>
          )}
        </div>
      )}
    </div>
  );
}

export default function ConnectorsPage() {
  const [active, setActive] = useState<ConnectorService | null>(null);
  const [ownApp, setOwnApp] = useState<ConnectorService | null>(null);
  const [detail, setDetail] = useState<ConnectorService | null>(null);

  if (detail) {
    return (
      <div className="mx-auto w-full max-w-2xl py-4">
        <ConnectorDetailPage
          svc={detail}
          onBack={() => setDetail(null)}
          onConnectKeys={setActive}
          onOwnApp={setOwnApp}
          onDisabled={() => {}}
        />
        <ConnectorConnectModal
          open={active !== null}
          service={active}
          onClose={() => setActive(null)}
          onSaved={() => {}}
        />
        <ConnectorConnectModal
          open={ownApp !== null}
          service={ownApp}
          mode="ownApp"
          onClose={() => setOwnApp(null)}
          onSaved={() => {}}
        />
      </div>
    );
  }

  return (
    <div className="mx-auto w-full max-w-2xl py-4">
      <h1 className="mb-1 text-2xl font-medium text-text-primary">
        Connectors
      </h1>
      <p className="mb-5 text-sm font-medium text-text-secondary">
        Connect Argus to your favourite tools.
      </p>
      <div className="mt-6">
        <h2
          className={
            "mb-1 px-2 text-xs font-medium uppercase tracking-wide " +
            "text-text-secondary"
          }
        >
          Browser
        </h2>
        <BrowserCard />
      </div>
      <div className="mt-5">
        <h2
          className={
            "mb-1 px-2 text-xs font-medium uppercase tracking-wide " +
            "text-text-secondary"
          }
        >
          Connectors
        </h2>
        <ConnectorGrid onOpen={setDetail} onConnectKeys={setActive} />
      </div>
      <ConnectorConnectModal
        open={active !== null}
        service={active}
        onClose={() => setActive(null)}
        onSaved={() => {}}
      />
      <ConnectorConnectModal
        open={ownApp !== null}
        service={ownApp}
        mode="ownApp"
        onClose={() => setOwnApp(null)}
        onSaved={() => {}}
      />
    </div>
  );
}
