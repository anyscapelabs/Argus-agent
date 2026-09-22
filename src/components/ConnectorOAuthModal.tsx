import { useEffect, useRef, useState } from "react";
import { LuX } from "react-icons/lu";
import { openUrl } from "@tauri-apps/plugin-opener";

import ConnectorIcon from "./ConnectorIcon";
import { useOAuthFlow, type OAuthSvc } from "../lib/oauthFlow";

const NOOP: OAuthSvc = {
  id: "",
  name: "",
  tagline: "",
  flow: "device",
  status: async () => ({ connected: false }),
  begin: async () => ({}),
  disconnect: async () => {},
};

type Props = {
  open: boolean;
  svc: OAuthSvc | null;
  onClose: () => void;
  onConnected: () => void;
};

export default function ConnectorOAuthModal({
  open,
  svc,
  onClose,
  onConnected,
}: Props) {
  const flow = useOAuthFlow(svc ?? NOOP);
  const [copied, setCopied] = useState(false);
  const started = useRef("");
  const done = useRef(false);

  useEffect(() => {
    if (!open || !svc) return;

    setCopied(false);
    done.current = false;

    if (started.current !== svc.id) {
      started.current = svc.id;
      void flow.connect();
    }
  }, [open, svc]);

  useEffect(() => {
    if (!open || !svc) return;

    if (flow.state === "waiting" && flow.verifyUrl && !done.current) {
      done.current = true;
      openUrl(flow.verifyUrl).catch(() => {});
    }

    if (flow.state === "connected") {
      started.current = "";
      onConnected();
      onClose();
    }
  }, [open, svc, flow.state, flow.verifyUrl]);

  if (!open || !svc) return null;

  const copy = async () => {
    try {
      await navigator.clipboard.writeText(flow.code);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch {}
  };

  const cancel = () => {
    started.current = "";
    flow.cancel();
    onClose();
  };

  return (
    <div
      className="fixed inset-0 z-[60] flex items-center justify-center bg-black/60 p-4"
      onClick={cancel}
      role="dialog"
      aria-modal="true"
      aria-label={`Connect ${svc.name}`}
    >
      <div
        className="flex w-full max-w-[480px] flex-col rounded-xl border border-border-primary bg-bg-secondary p-6 shadow-4xl"
        onClick={(evt) => evt.stopPropagation()}
      >
        <div className="flex items-center justify-end">
          <button
            type="button"
            onClick={cancel}
            aria-label="Close"
            className="flex h-6 w-6 items-center justify-center rounded text-text-secondary transition-colors hover:bg-bg-hover-secondary hover:text-text-primary"
          >
            <LuX size={14} />
          </button>
        </div>

        <div className="mt-1 flex items-center gap-2.5">
          <ConnectorIcon id={svc.id} size={22} />
          <h3 className="text-sm font-medium text-text-primary">
            Connect {svc.name}
          </h3>
        </div>

        <p className="mt-3 text-sm leading-relaxed text-text-secondary">
          {`Approve Argus in your browser to connect ${svc.name}. This closes by itself once you're done.`}
        </p>

        {flow.state === "busy" && (
          <p className="mt-4 text-sm text-text-secondary">
            Contacting {svc.name}…
          </p>
        )}

        {flow.state === "waiting" && flow.code && (
          <div className="mt-4 flex flex-col gap-2.5 rounded-lg border border-border-primary bg-bg-primary px-3 py-3">
            <p className="text-xs text-text-secondary">
              Enter this code on the {svc.name} page:
            </p>
            <div className="flex items-center gap-2">
              <span className="font-mono text-xl tracking-widest text-text-primary">
                {flow.code}
              </span>
              <button
                type="button"
                onClick={copy}
                className="rounded-md border border-border-primary px-2 py-1 text-xs text-text-secondary hover:text-text-primary cursor-pointer"
              >
                {copied ? "Copied ✓" : "Copy"}
              </button>
            </div>
            <button
              type="button"
              onClick={() => openUrl(flow.verifyUrl).catch(() => {})}
              className="mt-1 rounded-lg bg-accent px-3 py-1.5 text-sm font-medium text-bg-primary transition-opacity hover:opacity-90"
            >
              Open {svc.name} ↗
            </button>
            <p className="text-xs text-text-secondary">
              Approve there, then come back here.
            </p>
          </div>
        )}

        {flow.note && flow.state !== "waiting" && (
          <p className="mt-4 text-xs text-red-400">{flow.note}</p>
        )}

        <div className="mt-6 flex justify-end">
          <button
            type="button"
            onClick={cancel}
            className="rounded-lg border border-border-primary px-3 py-1.5 text-sm text-text-secondary hover:text-text-primary cursor-pointer"
          >
            Cancel
          </button>
        </div>
      </div>
    </div>
  );
}
