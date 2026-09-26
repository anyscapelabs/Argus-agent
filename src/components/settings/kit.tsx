import type { ReactNode } from "react";
import { LuChevronDown } from "react-icons/lu";

import { profileLabel } from "../../stores/profiles";

// One settings surface, one set of shapes. Every page is built from these, so a
// card here is the same size as a card there and a control here sits where a
// control sits everywhere else.

export function Page({ children }: { children: ReactNode }) {
  return <div className="flex flex-col gap-8">{children}</div>;
}

export function Card({
  children,
  flush = false,
}: {
  children: ReactNode;
  /// Rows divide themselves. Off for a card holding one block of text, where a
  /// divider under a single child would just be a line to nothing.
  flush?: boolean;
}) {
  return (
    <div
      className={
        "overflow-hidden rounded-xl border border-border-primary " +
        "bg-bg-secondary" +
        (flush ? "" : " divide-y divide-border-primary")
      }
    >
      {children}
    </div>
  );
}

export function Section({
  label,
  note,
  children,
}: {
  label: string;
  note?: string;
  children: ReactNode;
}) {
  return (
    <section className="flex flex-col">
      <div className="mb-3 flex flex-col">
        <h2 className="text-sm text-text-secondary">{label}</h2>
        {note !== undefined && (
          <p className="mt-1 text-xs leading-relaxed text-text-tertiary">
            {note}
          </p>
        )}
      </div>
      {children}
    </section>
  );
}

export function Row({
  title,
  desc,
  children,
  stacked = false,
}: {
  title: string;
  desc?: string;
  children?: ReactNode;
  /// A row taller than one line of label — a textarea, a grid. The control goes
  /// under the text instead of floating beside it.
  stacked?: boolean;
}) {
  if (stacked) {
    return (
      <div className="flex flex-col gap-3 px-4 py-4">
        <div className="flex flex-col">
          <h3 className="text-sm text-text-primary">{title}</h3>
          {desc !== undefined && (
            <p className="mt-0.5 text-xs leading-relaxed text-text-secondary">
              {desc}
            </p>
          )}
        </div>
        {children}
      </div>
    );
  }

  return (
    <div className="flex items-center gap-4 px-4 py-3.5">
      <div className="min-w-0 flex-1">
        <h3 className="text-sm text-text-primary">{title}</h3>
        {desc !== undefined && (
          <p className="mt-0.5 text-xs leading-relaxed text-text-secondary">
            {desc}
          </p>
        )}
      </div>
      {children !== undefined && (
        <div className="flex shrink-0 items-center gap-2">{children}</div>
      )}
    </div>
  );
}

export function Note({ children }: { children: ReactNode }) {
  return (
    <p className="text-xs leading-relaxed text-text-tertiary">{children}</p>
  );
}

const BTN =
  "inline-flex items-center justify-center gap-1.5 rounded-full px-3.5 " +
  "py-1.5 text-xs font-medium transition-opacity hover:opacity-90 " +
  "disabled:cursor-not-allowed disabled:opacity-40 disabled:hover:opacity-40";

export function Btn({
  children,
  onClick,
  variant = "secondary",
  disabled = false,
  title,
  className = "",
}: {
  children: ReactNode;
  onClick?: () => void;
  variant?: "primary" | "secondary" | "ghost" | "danger";
  disabled?: boolean;
  title?: string;
  className?: string;
}) {
  const look = {
    primary: "bg-accent text-bg-primary",
    secondary: "border border-border-primary text-text-primary",
    ghost: "text-text-secondary",
    danger: "border border-red-500/30 text-red-400",
  }[variant];

  return (
    <button
      type="button"
      onClick={onClick}
      disabled={disabled}
      title={title}
      className={BTN + " " + look + " " + className}
    >
      {children}
    </button>
  );
}

export const INPUT =
  "w-full rounded-lg border border-border-primary bg-bg-primary px-3 " +
  "py-2 text-sm text-text-primary outline-none placeholder:text-" +
  "tertiary focus:border-text-tertiary disabled:opacity-50";

type SegmentedProps<T extends string> = {
  value: T;
  opts: { value: T; label: string }[];
  onChange: (next: T) => void;
};

export function Segmented<T extends string>({
  value,
  opts,
  onChange,
}: SegmentedProps<T>) {
  return (
    <div
      className={
        "flex h-8 items-center rounded-full border border-border-primary " +
        "bg-bg-primary p-0.5 text-xs"
      }
    >
      {opts.map((opt) => {
        const active = opt.value === value;

        return (
          <button
            key={opt.value}
            type="button"
            onClick={() => onChange(opt.value)}
            className={
              "h-7 rounded-full px-3 font-medium transition-colors " +
              `${
                active
                  ? "bg-bg-hover-secondary text-text-primary"
                  : "text-text-secondary hover:text-text-primary"
              }`
            }
          >
            {opt.label}
          </button>
        );
      })}
    </div>
  );
}

type SelectProps<T extends string> = {
  value: T;
  opts: { value: T; label: string }[];
  onChange: (next: T) => void;
};

export function Select<T extends string>({
  value,
  opts,
  onChange,
}: SelectProps<T>) {
  return (
    <div className="relative">
      <select
        value={value}
        onChange={(e) => onChange(e.target.value as T)}
        className={
          "h-8 appearance-none rounded-full border border-border-primary " +
          "bg-bg-secondary pl-3 pr-8 text-xs text-text-primary outline-none " +
          "hover:bg-bg-hover-secondary"
        }
      >
        {opts.map((opt) => (
          <option key={opt.value} value={opt.value}>
            {opt.label}
          </option>
        ))}
      </select>
      <LuChevronDown
        size={12}
        className={
          "pointer-events-none absolute right-3 top-1/2 -translate-y-1/2 " +
          "text-text-tertiary"
        }
      />
    </div>
  );
}

type ToggleProps = {
  checked: boolean;
  onChange: (next: boolean) => void;
  label: string;
};

export function Toggle({ checked, onChange, label }: ToggleProps) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      onClick={() => onChange(!checked)}
      className={
        "relative h-5 w-9 shrink-0 rounded-full transition-colors " +
        `${checked ? "bg-accent" : "bg-bg-hover-secondary"}`
      }
    >
      <span
        className={
          "absolute top-0.5 h-4 w-4 rounded-full bg-bg-primary " +
          "transition-all " +
          `${checked ? "left-[18px]" : "left-0.5"}`
        }
      />
    </button>
  );
}

export function ProfileAvatar({
  name,
  size = 28,
}: {
  name: string;
  size?: number;
}) {
  return (
    <span
      className={
        "flex shrink-0 items-center justify-center rounded-full " +
        "bg-bg-hover-secondary font-medium text-text-secondary"
      }
      style={{ width: size, height: size, fontSize: size * 0.4 }}
    >
      {profileLabel({ name }).charAt(0).toUpperCase()}
    </span>
  );
}
