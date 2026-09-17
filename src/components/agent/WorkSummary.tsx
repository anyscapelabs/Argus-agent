import { useState } from "react";
import { FiChevronDown } from "react-icons/fi";

type Props = {
  label: string;
  children?: React.ReactNode;
};

export default function WorkSummary({ label, children }: Props) {
  const [open, setOpen] = useState(false);

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
        <span>{label}</span>
        <FiChevronDown
          size={14}
          className={`shrink-0 transition-transform ${open ? "" : "-rotate-90"}`}
        />
      </button>
      {open && <div className="mt-2 pl-4">{children}</div>}
    </div>
  );
}

export function parseDbTime(s: string | null | undefined): number | null {
  if (!s) {
    return null;
  }

  const t = /^\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}$/.test(s)
    ? Date.parse(`${s.replace(" ", "T")}Z`)
    : Date.parse(s);

  return Number.isNaN(t) ? null : t;
}

export function formatWorked(startMs: number | null, endMs: number | null): string {
  if (startMs === null || endMs === null || endMs < startMs) {
    return "Work details";
  }

  const totalSec = Math.max(1, Math.round((endMs - startMs) / 1000));

  if (totalSec < 60) {
    return `Worked for ${totalSec} sec`;
  }

  const mins = Math.floor(totalSec / 60);
  const secs = totalSec % 60;

  if (mins < 60) {
    return secs === 0
      ? `Worked for ${mins} min`
      : `Worked for ${mins} min ${secs} sec`;
  }

  const hours = Math.floor(mins / 60);
  const remMin = mins % 60;

  return remMin === 0
    ? `Worked for ${hours} hr`
    : `Worked for ${hours} hr ${remMin} min`;
}
