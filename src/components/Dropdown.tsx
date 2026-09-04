import { useEffect, useRef, useState, type ReactNode } from "react";
import { FiCheck, FiChevronRight } from "react-icons/fi";

export type DropdownItem = {
  label: string;
  Icon?: React.ComponentType<{ size?: number; className?: string }>;
  onClick?: () => void;
  disabled?: boolean;
  danger?: boolean;
  hasSubmenu?: boolean;
  toggleable?: boolean;
  active?: boolean;
};

type Props = {
  trigger: (props: { open: boolean; toggle: () => void }) => ReactNode;
  items: DropdownItem[];
  align?: "left" | "right";
  side?: "top" | "bottom";
  panelClassName?: string;
};

export default function Dropdown({ trigger, items, align = "left", side = "top", panelClassName }: Props) {
  const [open, setOpen] = useState(false);
  const rootRef = useRef<HTMLDivElement | null>(null);
  const [activeMap, setActiveMap] = useState<Record<number, boolean>>({});

  useEffect(() => {
    if (!open) return;
    const onDoc = (e: MouseEvent) => {
      if (rootRef.current && !rootRef.current.contains(e.target as Node)) {
        setOpen(false);
      }
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setOpen(false);
    };
    document.addEventListener("mousedown", onDoc);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onDoc);
      document.removeEventListener("keydown", onKey);
    };
  }, [open]);

  const toggle = () => setOpen((v) => !v);
  const close = () => setOpen(false);

  return (
    <div ref={rootRef} className="relative">
      {trigger({ open, toggle })}
      {open && (
        <div
          role="menu"
          className={`absolute z-50 min-w-[220px] overflow-hidden rounded-2xl border border-border-primary bg-bg-secondary p-1 shadow-4xl ${
            side === "top" ? "bottom-full mb-2" : "top-full mt-2"
          } ${
            align === "right" ? "right-0" : "left-0"
          } ${panelClassName ?? ""}`}
        >
          {items.map((it, i) => {
            const Icon = it.Icon;
            const isActive = it.toggleable === true && (activeMap[i] ?? it.active === true);
            return (
              <div key={`${it.label}-${i}`}>
                {i > 0 && <div className="my-1 h-px bg-border-primary" />}
                <button
                  type="button"
                  role="menuitem"
                  disabled={it.disabled}
                  onClick={() => {
                    if (it.toggleable === true) {
                      setActiveMap((prev) => ({ ...prev, [i]: !(prev[i] ?? it.active === true) }));
                    }
                    it.onClick?.();
                    if (it.toggleable !== true) close();
                  }}
                  className={`flex w-full items-center gap-2 rounded-xl px-2 py-1 text-left text-sm transition-colors hover:bg-bg-hover-secondary focus:outline-none focus-visible:bg-bg-hover-secondary disabled:cursor-not-allowed disabled:opacity-50 ${
                    it.danger ? "text-red-400 hover:text-red-300" : "text-text-primary"
                  }`}
                >
                  {Icon !== undefined && <Icon size={16} className="shrink-0" />}
                  <span className="flex-1 truncate">{it.label}</span>
                  {it.hasSubmenu === true && <FiChevronRight size={14} className="shrink-0 text-text-secondary" />}
                  {isActive && <FiCheck size={14} className="shrink-0 text-blue-400" />}
                </button>
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}
