import { useEffect, useRef, useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";

import {
  githubConnect,
  githubDisconnect,
  githubStatus,
  googleConnectUrl,
  googleDisconnect,
  googleStatus,
  outlookConnect,
  outlookDisconnect,
  outlookStatus,
  spotifyConnectUrl,
  spotifyDisconnect,
  spotifyStatus,
} from "../lib/ipc";
import ConnectorIcon from "./ConnectorIcon";

type Flow = "loopback" | "device";

type Svc = {
  id: string;
  name: string;
  tagline: string;
  flow: Flow;
  status: () => Promise<{ connected: boolean; email?: string | null; login?: string | null }>;
  begin: () => Promise<{ url?: string; verificationUri?: string; userCode?: string }>;
  disconnect: () => Promise<void>;
};

const SERVICES: Svc[] = [
  {
    id: "google",
    name: "Google",
    tagline: "Gmail, Calendar, Drive, Docs, Sheets",
    flow: "loopback",
    status: googleStatus,
    begin: async () => googleConnectUrl(),
    disconnect: googleDisconnect,
  },
  {
    id: "github",
    name: "GitHub",
    tagline: "Repos, issues, PRs and Actions",
    flow: "device",
    status: githubStatus,
    begin: async () => githubConnect(),
    disconnect: githubDisconnect,
  },
  {
    id: "outlook",
    name: "Outlook",
    tagline: "Mail and calendar",
    flow: "device",
    status: outlookStatus,
    begin: async () => outlookConnect(),
    disconnect: outlookDisconnect,
  },
  {
    id: "spotify",
    name: "Spotify",
    tagline: "Now playing and playback",
    flow: "loopback",
    status: spotifyStatus,
    begin: async () => spotifyConnectUrl(),
    disconnect: spotifyDisconnect,
  },
];

const POLL_MS = 2_000;
const WAIT_MS = 5 * 60_000;

type Props = {
  svc: Svc;
  onOwnApp: (svc: Svc) => void;
};

export default function ConnectorOAuthCard({ svc, onOwnApp }: Props) {
  const [state, setState] = useState<"off" | "busy" | "waiting" | "connected">("off");
  const [who, setWho] = useState("");
  const [code, setCode] = useState("");
  const [verifyUrl, setVerifyUrl] = useState("");
  const [note, setNote] = useState("");
  const alive = useRef(true);

  useEffect(() => {
    alive.current = true;

    svc
      .status()
      .then((s) => {
        if (!alive.current) return;

        if (s.connected) {
          setState("connected");
          setWho(String(s.email ?? s.login ?? ""));
        }
      })
      .catch(() => {});

    return () => {
      alive.current = false;
    };
  }, [svc]);

  useEffect(() => {
    if (state !== "waiting") return;

    const started = Date.now();
    const poll = setInterval(async () => {
      if (Date.now() - started > WAIT_MS) {
        clearInterval(poll);
        setState("off");
        setCode("");
        setNote("Timed out — click Connect to retry.");
        return;
      }

      try {
        const s = await svc.status();

        if (s.connected) {
          clearInterval(poll);
          setState("connected");
          setWho(String(s.email ?? s.login ?? ""));
          setCode("");
          setNote("");
        }
      } catch {}
    }, POLL_MS);

    return () => clearInterval(poll);
  }, [state, svc]);

  const connect = async () => {
    setState("busy");
    setNote("");

    try {
      const d = await svc.begin();

      if (d.url) {
        await openUrl(d.url);
        setNote("Approve in your browser, then come back here.");
      }

      if (d.verificationUri && d.userCode) {
        setCode(d.userCode);
        setVerifyUrl(d.verificationUri);
        setNote("Enter the code at the link, then come back here.");
      }

      setState("waiting");
    } catch (err) {
      setState("off");
      setNote(String(err));
    }
  };

  const cancel = () => {
    setState("off");
    setCode("");
    setVerifyUrl("");
    setNote("");
  };

  const line =
    state === "connected"
      ? who || "Connected"
      : state === "waiting"
        ? note
        : state === "busy"
          ? "Opening…"
          : note || svc.tagline;

  return (
    <div className="flex flex-col rounded-xl bg-transparent px-2 py-1 transition-colors hover:bg-bg-hover-primary">
      <div className="flex items-center gap-3">
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
          <p className="truncate text-xs text-text-secondary">{line}</p>
        </div>
        {state === "connected" ? (
          <span
            className={
              "shrink-0 rounded-full bg-green-600/15 px-3 py-1 text-xs " +
              "font-medium text-green-500"
            }
          >
            Connected
          </span>
        ) : state === "waiting" ? (
          <button
            type="button"
            onClick={cancel}
            className="shrink-0 rounded-full border border-border-primary px-3 py-1 text-xs text-text-secondary hover:text-text-primary cursor-pointer"
          >
            Cancel
          </button>
        ) : (
          <button
            type="button"
            onClick={connect}
            disabled={state === "busy"}
            className={
              "shrink-0 rounded-full border border-border-primary " +
              "bg-bg-hover-secondary px-3 py-1 text-xs font-medium " +
              "text-text-primary transition-colors " +
              "hover:bg-bg-hover-primary cursor-pointer disabled:opacity-60"
            }
          >
            Connect
          </button>
        )}
      </div>
      {state === "waiting" && code && (
        <div className="ml-14 mt-1">
          <button
            type="button"
            onClick={() => openUrl(verifyUrl).catch(() => {})}
            className={
              "rounded-lg border border-border-primary bg-bg-secondary " +
              "px-2.5 py-1 font-mono text-sm tracking-widest " +
              "text-text-primary hover:bg-bg-hover-primary cursor-pointer"
            }
          >
            {code}
          </button>
        </div>
      )}
      <div className="ml-14 mt-0.5">
        <button
          type="button"
          onClick={() => onOwnApp(svc)}
          className="text-[11px] text-text-secondary/70 hover:text-text-primary cursor-pointer"
        >
          Use my own OAuth app
        </button>
      </div>
    </div>
  );
}

export const OAUTH_SERVICES = SERVICES;
