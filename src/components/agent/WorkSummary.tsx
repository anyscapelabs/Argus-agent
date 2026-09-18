import { useEffect, useRef } from "react";
import { useState } from "react";
import { FiChevronDown } from "react-icons/fi";

import { useWorkTimer } from "../../stores/workTimer";

type Props = {
  label: string;
  live?: boolean;
  startedAt?: number | null;
  children?: React.ReactNode;
};

export function formatDuration(ms: number): string {
  const totalSec = Math.max(0, Math.round(ms / 1000));

  if (totalSec < 60) {
    return `${totalSec} sec`;
  }

  const mins = Math.floor(totalSec / 60);
  const secs = totalSec % 60;

  if (mins < 60) {
    return secs === 0 ? `${mins} min` : `${mins} min ${secs} sec`;
  }

  const hours = Math.floor(mins / 60);
  const remMin = mins % 60;

  return remMin === 0 ? `${hours} hr` : `${hours} hr ${remMin} min`;
}

export default function WorkSummary({ label, live, startedAt, children }: Props) {
  const [open, setOpen] = useState(false);
  const now = useWorkTimer((s) => s.now);
  const mountedAt = useRef(Date.now());

  useEffect(() => {
    if (!live) {
      return;
    }

    useWorkTimer.getState().start();

    return () => {
      useWorkTimer.getState().stop();
    };
  }, [live]);

  const base = startedAt ?? mountedAt.current;
  const header = live ? `Working… ${formatDuration(now - base)}` : label;

  return (
    <div className="font-sans">
      <button
        type="button"
        onClick={() => setOpen((v) => !v)}
        className={
          "flex w-fit items-center gap-1.5 text-[14px] text-text-secondary " +
          "transition-colors hover:text-text-primary focus:outline-none " +
          "focus-visible:text-text-primary"
        }
        aria-expanded={open}
      >
        <span>{header}</span>
        <FiChevronDown
          size={14}
          className={`shrink-0 transition-transform ${open ? "" : "-rotate-90"}`}
        />
      </button>
      {open && <div className="mt-2 pl-4">{children}</div>}
    </div>
  );
}

export function formatWorked(startMs: number | null, endMs: number | null): string {
  if (startMs === null || endMs === null || endMs < startMs) {
    return "Work details";
  }

  return `Worked for ${formatDuration(endMs - startMs)}`;
}
