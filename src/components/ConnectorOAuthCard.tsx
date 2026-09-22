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
import { useOAuthFlow, type OAuthSvc } from "../lib/oauthFlow";
import ConnectorIcon from "./ConnectorIcon";

const SERVICES: OAuthSvc[] = [
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

export const OAUTH_SERVICES = SERVICES;

export function oauthSvcById(id: string): OAuthSvc | undefined {
  return SERVICES.find((s) => s.id === id);
}

type Props = {
  svc: OAuthSvc;
  onOpen: (id: string) => void;
};

export default function ConnectorOAuthCard({ svc, onOpen }: Props) {
  const flow = useOAuthFlow(svc);
  const { state, code, verifyUrl, note } = flow;
  const [copied, setCopied] = useState(false);
  const opened = useRef(false);

  useEffect(() => {
    if (state === "waiting") {
      setCopied(false);

      if (verifyUrl && !opened.current) {
        opened.current = true;
        openUrl(verifyUrl).catch(() => {});
      }
    } else {
      opened.current = false;
    }
  }, [state, verifyUrl]);

  const copy = async () => {
    try {
      await navigator.clipboard.writeText(code);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch {}
  };

  const line =
    state === "connected"
      ? svc.tagline
      : state === "waiting"
        ? note
        : state === "busy"
          ? "Opening…"
          : note || svc.tagline;

  return (
    <div
      onClick={() => onOpen(svc.id)}
      className="flex cursor-pointer flex-col rounded-xl bg-transparent px-2 py-1 transition-colors hover:bg-bg-hover-primary"
    >
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
            onClick={(e) => {
              e.stopPropagation();
              onOpen(svc.id);
            }}
            className={
              "shrink-0 rounded-md bg-green-600 px-3 py-1 text-xs " +
              "font-medium text-white cursor-pointer"
            }
          >
            Connected
          </span>
        ) : state === "waiting" ? (
          <button
            type="button"
            onClick={(e) => {
              e.stopPropagation();
              flow.cancel();
            }}
            className="shrink-0 rounded-md border border-border-primary px-3 py-1 text-xs text-text-secondary hover:text-text-primary cursor-pointer"
          >
            Cancel
          </button>
        ) : (
          <button
            type="button"
            onClick={(e) => {
              e.stopPropagation();
              void flow.connect();
            }}
            disabled={state === "busy"}
            className={
              "shrink-0 rounded-md border border-border-primary " +
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
        <div
          className="ml-14 mt-2 flex flex-col gap-1.5 rounded-md border border-border-primary bg-bg-secondary/50 px-2.5 py-2"
          onClick={(e) => e.stopPropagation()}
        >
          <p className="text-[11px] leading-snug text-text-secondary">
            Copy the code, open {svc.name}, and paste it there:
          </p>
          <div className="flex items-center gap-2">
            <span className="font-mono text-sm tracking-widest text-text-primary">
              {code}
            </span>
            <button
              type="button"
              onClick={copy}
              className="rounded-md border border-border-primary px-2 py-0.5 text-[11px] text-text-secondary hover:text-text-primary cursor-pointer"
            >
              {copied ? "Copied ✓" : "Copy"}
            </button>
            <button
              type="button"
              onClick={() => openUrl(verifyUrl).catch(() => {})}
              className="rounded-md bg-accent px-2 py-0.5 text-[11px] font-medium text-bg-primary hover:opacity-90 cursor-pointer"
            >
              Open {svc.name} ↗
            </button>
          </div>
          <p className="text-[11px] leading-snug text-text-secondary">
            Approve there, then come back — this flips to Connected by itself.
          </p>
        </div>
      )}
    </div>
  );
}
