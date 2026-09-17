import { create } from "zustand";

type WorkTimerState = {
  now: number;
  start: () => void;
  stop: () => void;
};

let timer: ReturnType<typeof setInterval> | null = null;
let refs = 0;

function ensureTicking() {
  if (timer !== null) {
    return;
  }

  timer = setInterval(() => {
    useWorkTimer.setState({ now: Date.now() });
  }, 1000);
}

function maybeStop() {
  if (refs === 0 && timer !== null) {
    clearInterval(timer);
    timer = null;
  }
}

export const useWorkTimer = create<WorkTimerState>()(() => ({
  now: Date.now(),

  start: () => {
    refs += 1;
    ensureTicking();
  },

  stop: () => {
    refs = Math.max(0, refs - 1);
    maybeStop();
  },
}));
