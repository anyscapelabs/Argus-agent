import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
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

import { sessBrowserImport } from "../lib/ipc";
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

type ImportState = "idle" | "busy" | "ok" | "err";

function BrowserCard() {
  const [state, setState] = useState<ImportState>("idle");
  const [note, setNote] = useState("");

  useEffect(() => {
    const un = listen("browser-import-done", (e) => {
      const p = e.payload as { Ok?: string; Err?: string } | string | null;

      if (typeof p === "string") {
        setState("ok");
        setNote(p);
        return;
      }

      if (p && p.Err) {
        setState("err");
        setNote(p.Err);
        return;
      }

      setState("ok");
      setNote(p?.Ok ?? "Profile imported");
    });

    return () => {
      void un.then((f) => f());
    };
  }, []);

  const run = async () => {
    setState("busy");
    setNote("");

    try {
      await sessBrowserImport("main");
    } catch (err) {
      setState("err");
      setNote(String(err));
    }
  };

  const busy = state === "busy";

  return (
    <div
      className={
        "flex items-center gap-4 rounded-xl bg-transparent px-2 py-1 " +
        "transition-colors hover:bg-bg-hover-primary"
      }
    >
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
        <h3 className="truncate text-sm font-medium text-text-primary">
          Browser
        </h3>
        <p className="truncate text-xs text-text-secondary">
          {note || "Import your Chrome profile so logins carry over. Close Chrome first."}
        </p>
      </div>
      <button
        type="button"
        onClick={run}
        disabled={busy}
        className={
          "flex shrink-0 items-center gap-1.5 rounded-full border " +
          "border-border-primary bg-bg-hover-secondary px-3 py-1.5 " +
          "text-xs font-medium text-text-primary transition-colors " +
          "hover:bg-bg-hover-primary cursor-pointer " +
          "disabled:cursor-default disabled:opacity-60"
        }
      >
        {busy && <LuLoaderCircle size={12} className="animate-spin" />}
        Import Chrome profile
      </button>
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
