import { create } from "zustand";

import { voiceCancel, voiceStatus, voiceTranscribe } from "../lib/ipc";
import { encodeWav, MAX_SECS, Recorder, TARGET_RATE } from "../lib/recorder";

type Phase = "idle" | "listening" | "working" | "error";

type VoiceState = {
  phase: Phase;
  level: number;
  elapsed: number;
  err: string | null;
  start: () => Promise<void>;
  stop: () => Promise<string | null>;
  cancel: () => void;
  reset: () => void;
};

// Held outside the store because they are resources, not state: a React render
// has no business owning a microphone.
let recorder: Recorder | null = null;
let ticker: ReturnType<typeof setInterval> | null = null;

function clearTicker() {
  if (ticker !== null) {
    clearInterval(ticker);
    ticker = null;
  }
}

// getUserMedia failures are opaque by default, and Linux has no permission
// prompt at all, so the browser's name is the only thing the user has to go on.
function micWhy(e: unknown): string {
  const name = e instanceof DOMException ? e.name : "";

  if (name === "NotAllowedError") {
    return "Argus was not allowed to use the microphone.";
  }
  if (name === "NotFoundError" || name === "OverconstrainedError") {
    return "No microphone was found.";
  }
  if (name === "NotReadableError") {
    return "Another app is using the microphone.";
  }

  return e instanceof Error ? e.message : String(e);
}

async function ensureModel(): Promise<void> {
  const status = await voiceStatus();

  if (status.installed) {
    return;
  }

  throw new Error("The voice model is missing. Reinstall Argus, or pick another in Settings.");
}

export const useVoice = create<VoiceState>()((set, get) => ({
  phase: "idle",
  level: 0,
  elapsed: 0,
  err: null,

  start: async () => {
    if (get().phase !== "idle") return;

    set({ err: null, level: 0, elapsed: 0 });

    // The model shipped inside the app, so there is nothing to fetch here and
    // nothing to wait for. This only fires if the install is broken, and it
    // fails closed rather than starting a download nobody asked for.
    try {
      await ensureModel();
    } catch (e) {
      set({ phase: "error", err: e instanceof Error ? e.message : String(e) });
      return;
    }

    const rec = new Recorder();

    try {
      await rec.start((level) => set({ level }));
    } catch (e) {
      rec.cancel();
      set({ phase: "error", err: micWhy(e) });
      return;
    }

    recorder = rec;
    const startedAt = Date.now();
    set({ phase: "listening" });

    ticker = setInterval(() => {
      const elapsed = Math.floor((Date.now() - startedAt) / 1000);

      // The button only shows whole seconds, so the render that would not
      // change anything is skipped.
      if (elapsed === get().elapsed) return;

      set({ elapsed });

      if (elapsed >= MAX_SECS) {
        void get().stop();
      }
    }, 200);
  },

  // Returns the transcript for the caller to place in the composer. It is
  // never sent: sending is the user's thumb on Enter, and a misheard word in a
  // sent message is an instruction that ran.
  stop: async () => {
    if (get().phase !== "listening" || recorder === null) return null;

    clearTicker();
    const { pcm } = recorder.take();
    recorder = null;
    set({ phase: "working", level: 0, elapsed: 0 });

    if (pcm.length === 0) {
      set({ phase: "idle" });
      return null;
    }

    try {
      const text = await voiceTranscribe(encodeWav(pcm, TARGET_RATE));
      set({ phase: "idle" });
      return text.trim().length > 0 ? text : null;
    } catch (e) {
      set({ phase: "error", err: e instanceof Error ? e.message : String(e) });
      return null;
    }
  },

  cancel: () => {
    clearTicker();
    recorder?.cancel();
    recorder = null;

    // A decode already running keeps going in the native engine; this is what
    // actually stops it rather than throwing the result away.
    void voiceCancel();
    set({ phase: "idle", level: 0, elapsed: 0 });
  },

  reset: () => set({ phase: "idle", err: null }),
}));