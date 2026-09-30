import { create } from "zustand";

import { voiceCancel, voiceListening, voiceTranscribe } from "../lib/ipc";
import { encodeWav, MAX_SECS, Recorder, TARGET_RATE } from "../lib/recorder";
import { toast } from "./toast";

type Phase = "idle" | "listening" | "working";

// Three phases and no error phase: the model ships in the installer, so the
// mic either records or it does not, and there is nothing to report in the
// box. Both failure sentences — a device the OS refused, a clip that held no
// words — go to a toast, where the rest of the app says them.

type VoiceState = {
  phase: Phase;
  level: number;
  elapsed: number;
  start: () => Promise<void>;
  stop: () => Promise<string | null>;
  cancel: () => void;
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

// Every path out of a recording goes through here, so the grant is never left
// open by a take that ended early, was cancelled, or failed to start.
function closeMic(): void {
  void voiceListening(false);
}

export const useVoice = create<VoiceState>()((set, get) => ({
  phase: "idle",
  level: 0,
  elapsed: 0,

  start: async () => {
    if (get().phase !== "idle") return;

    set({ level: 0, elapsed: 0 });

    // Opened before the recorder, or the grant is still closed when the
    // webview asks for it.
    await voiceListening(true);

    const rec = new Recorder();

    try {
      await rec.start((level) => set({ level }));
    } catch (e) {
      rec.cancel();
      closeMic();
      toast.error(micWhy(e));
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
    closeMic();
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
      set({ phase: "idle" });
      toast.error(e instanceof Error ? e.message : String(e));
      return null;
    }
  },

  cancel: () => {
    clearTicker();
    recorder?.cancel();
    recorder = null;
    closeMic();

    // A decode already running keeps going in the native engine; this is what
    // actually stops it rather than throwing the result away.
    void voiceCancel();
    set({ phase: "idle", level: 0, elapsed: 0 });
  },
}));