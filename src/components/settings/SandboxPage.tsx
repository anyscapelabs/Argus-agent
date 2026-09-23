import { useCallback, useEffect, useState } from "react";

import {
  sandboxConfig,
  sandboxSetConfig,
  type SandboxConfig,
} from "../../lib/ipc";

const INPUT =
  "w-full rounded-lg border border-border-primary bg-bg-hover-secondary " +
  "px-2 py-1.5 font-mono text-xs text-text-primary " +
  "placeholder:text-text-secondary focus:outline-none " +
  "focus:ring-1 focus:ring-text-secondary";

export default function SandboxPage() {
  const [cfg, setCfg] = useState<SandboxConfig | null>(null);
  const [hosts, setHosts] = useState("");
  const [images, setImages] = useState("");
  const [def, setDef] = useState("");
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  const [note, setNote] = useState("");

  const refresh = useCallback(async () => {
    try {
      const next = await sandboxConfig();
      setCfg(next);
      setHosts(next.hosts.join("\n"));
      setImages(next.images.join("\n"));
      setDef(next.defaultImage);
      setErr(null);
    } catch (e) {
      setErr(e instanceof Error ? e.message : "sandbox config failed");
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const save = async () => {
    setBusy(true);
    setErr(null);
    setNote("");

    try {
      const next = await sandboxSetConfig(
        hosts.split("\n").map((s) => s.trim()).filter((s) => s !== ""),
        images.split("\n").map((s) => s.trim()).filter((s) => s !== ""),
        def.trim(),
      );
      setCfg(next);
      setNote("Saved — code.run uses these images, offline only.");
    } catch (e) {
      setErr(e instanceof Error ? e.message : "save failed");
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="flex flex-col gap-4 px-2 py-3">
      <div>
        <h2 className="text-sm font-medium text-text-primary">Sandbox</h2>
        <p className="text-xs text-text-secondary">
          Untrusted code runs offline in a container (read-only root, no
          capabilities, 2 CPU / 2 GB). Nothing runs unless its image is listed
          here with a digest pin.
        </p>
      </div>

      <label className="flex flex-col gap-1.5">
        <span className="text-sm text-text-secondary">
          Images <span className="text-text-secondary/70">(one per line, name:tag@sha256:…)</span>
        </span>
        <textarea
          value={images}
          rows={4}
          placeholder={"docker.io/library/python:3.12@sha256:…"}
          onChange={(e) => setImages(e.target.value)}
          className={INPUT + " resize-y"}
        />
      </label>

      <label className="flex flex-col gap-1.5">
        <span className="text-sm text-text-secondary">
          Default image <span className="text-text-secondary/70">(must be one of the above)</span>
        </span>
        <input
          type="text"
          value={def}
          placeholder="docker.io/library/python:3.12@sha256:…"
          autoComplete="off"
          onChange={(e) => setDef(e.target.value)}
          className={INPUT}
        />
      </label>

      <label className="flex flex-col gap-1.5">
        <span className="text-sm text-text-secondary">
          Trusted hosts <span className="text-text-secondary/70">(one per line — clones and fetches from these count as trusted)</span>
        </span>
        <textarea
          value={hosts}
          rows={3}
          placeholder={"github.com\n"}
          onChange={(e) => setHosts(e.target.value)}
          className={INPUT + " resize-y"}
        />
      </label>

      {err && <p className="text-xs text-red-400">{err}</p>}
      {note && <p className="text-xs text-green-500">{note}</p>}

      <div className="flex items-center gap-3">
        <button
          type="button"
          onClick={save}
          disabled={busy}
          className={
            "rounded-md bg-accent px-3 py-1.5 text-sm font-medium " +
            "text-bg-primary transition-opacity hover:opacity-90 " +
            "disabled:opacity-50"
          }
        >
          {busy ? "Saving…" : "Save"}
        </button>
        {cfg && (
          <span className="text-xs text-text-secondary">
            {cfg.images.length} image{cfg.images.length === 1 ? "" : "s"} ·{" "}
            {cfg.hosts.length} host{cfg.hosts.length === 1 ? "" : "s"}
          </span>
        )}
      </div>
    </div>
  );
}
