import { useCallback, useEffect, useRef, useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";

export type OAuthSvc = {
  id: string;
  name: string;
  tagline: string;
  flow: "loopback" | "device";
  status: () => Promise<{
    connected: boolean;
    email?: string | null;
    login?: string | null;
  }>;
  begin: () => Promise<{
    url?: string;
    verificationUri?: string;
    userCode?: string;
  }>;
  disconnect: () => Promise<void>;
};

export type FlowState = "off" | "busy" | "waiting" | "connected";

const POLL_MS = 2_000;
const WAIT_MS = 5 * 60_000;

export function useOAuthFlow(svc: OAuthSvc) {
  const [state, setState] = useState<FlowState>("off");
  const [who, setWho] = useState("");
  const [code, setCode] = useState("");
  const [verifyUrl, setVerifyUrl] = useState("");
  const [note, setNote] = useState("");
  const [tick, setTick] = useState(0);
  const alive = useRef(true);

  const refresh = useCallback(() => {
    setTick((t) => t + 1);
  }, []);

  useEffect(() => {
    alive.current = true;

    svc
      .status()
      .then((s) => {
        if (!alive.current) return;

        if (s.connected) {
          setState("connected");
          setWho(String(s.email ?? s.login ?? ""));
        } else {
          setState("off");
          setWho("");
        }
      })
      .catch(() => {});

    return () => {
      alive.current = false;
    };
  }, [svc, tick]);

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

  const connect = useCallback(async () => {
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
  }, [svc]);

  const cancel = useCallback(() => {
    setState("off");
    setCode("");
    setVerifyUrl("");
    setNote("");
  }, []);

  const drop = useCallback(async () => {
    await svc.disconnect();
    setState("off");
    setWho("");
    setNote("");
  }, [svc]);

  return { state, who, code, verifyUrl, note, connect, cancel, drop, refresh };
}
