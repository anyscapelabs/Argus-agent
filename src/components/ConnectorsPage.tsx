import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";
import { LuGlobe, LuLoaderCircle } from "react-icons/lu";
import {
  LuCalendar,
  LuDatabase,
  LuGithub,
  LuHardDrive,
  LuMail,
  LuMessageSquare,
  LuSearch,
} from "react-icons/lu";

import {
  sessBrowserImport,
  sessExtInstall,
  sessExtStatus,
  sessExtUninstall,
} from "../lib/ipc";
import ConnectorCard, { type Connector } from "./ConnectorCard";

const CONNECTORS: Connector[] = [
  {
    id: "gmail",
    name: "Gmail",
    description: "Connect your Gmail inbox",
    icon: <LuMail />,
  },
  {
    id: "github",
    name: "GitHub",
    description: "Sync repositories & PRs",
    icon: <LuGithub />,
  },
  {
    id: "calendar",
    name: "Google Calendar",
    description: "Manage events & meetings",
    icon: <LuCalendar />,
  },
  {
    id: "drive",
    name: "Google Drive",
    description: "Access docs & files",
    icon: <LuHardDrive />,
  },
  {
    id: "slack",
    name: "Slack",
    description: "Collaborate with your team",
    icon: <LuMessageSquare />,
  },
  {
    id: "notion",
    name: "Notion",
    description: "Sync notes & databases",
    icon: <LuDatabase />,
  },
];

type ExtState = "off" | "busy" | "granted" | "connected";
type ImportState = "idle" | "busy" | "ok" | "err";

const ON_KEY = "argus.ext.enabled";

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
        // status check failing isn't fatal, keep polling
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
      // host manifest already gone is fine
    }
  };

  const toggle = () => {
    if (extState === "busy") return;
    if (extState === "off") void enable();
    if (extState !== "off") void disable();
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
      ? "Connected — the agent can use your real Chrome."
      : extState === "granted"
        ? note
        : extState === "busy"
          ? "Saving permission…"
          : "Permission for the agent to act inside your real Chrome. Chrome only opens when it needs to.";

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
          <LuGlobe />
        </div>
        <div className="flex-1 min-w-0">
          <div className="flex items-center gap-2">
            <h3 className="truncate text-sm font-medium text-text-primary">
              Chrome — real browser
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
          aria-label="Allow the agent to use real Chrome"
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
              "flex items-center gap-1.5 rounded-full border " +
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
              "flex items-center gap-1.5 rounded-full border " +
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
  return (
    <div className="mx-auto w-full max-w-2xl py-4">
      <h1 className="mb-1 text-2xl font-medium text-text-primary">
        Connectors
      </h1>
      <p className="mb-5 text-sm font-medium text-text-secondary">
        Connect Argus to your favourite tools.
      </p>
      <div
        className={
          "flex h-10 w-full items-center rounded-full border " +
          "border-border-primary bg-bg-secondary px-4"
        }
      >
        <LuSearch size={18} className="shrink-0 text-text-secondary" />
        <input
          type="text"
          placeholder="Search connectors..."
          className={
            "ml-2 flex-1 bg-transparent text-sm text-text-primary " +
            "outline-none placeholder:text-text-secondary"
          }
        />
      </div>
      <div className="mt-6">
        <BrowserCard />
      </div>
      <div className="mt-2 grid grid-cols-2 gap-4">
        {CONNECTORS.map((connector) => (
          <ConnectorCard key={connector.id} connector={connector} />
        ))}
      </div>
    </div>
  );
}
