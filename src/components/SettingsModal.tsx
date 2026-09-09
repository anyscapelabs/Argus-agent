import { useEffect, useState, type ReactNode } from "react";

import ModelsPage from "./settings/ModelsPage";
import ProvidersPage from "./settings/ProvidersPage";
import SettingsSidebar, { type SettingsTab } from "./settings/SettingsSidebar";

type SettingsCardProps = {
  title: string;
  description: string;
  control: ReactNode;
};

function SettingsCard({ title, description, control }: SettingsCardProps) {
  return (
    <div
      className={
        "flex items-center gap-4 rounded-xl bg-transparent px-2 py-3 " +
        "transition-colors hover:bg-bg-hover-primary"
      }
    >
      <div className="min-w-0 flex-1">
        <h3 className="truncate text-sm font-medium text-text-primary">
          {title}
        </h3>
        <p className="truncate text-xs text-text-secondary">{description}</p>
      </div>
      <div className="shrink-0">{control}</div>
    </div>
  );
}

type SegmentedProps<T extends string> = {
  value: T;
  options: { value: T; label: string }[];
  onChange: (next: T) => void;
};

function Segmented<T extends string>({
  value,
  options,
  onChange,
}: SegmentedProps<T>) {
  return (
    <div
      className={
        "flex h-7 items-center rounded-full border border-border-primary " +
        "bg-bg-secondary p-0.5 text-xs"
      }
    >
      {options.map((opt) => {
        const active = opt.value === value;

        return (
          <button
            key={opt.value}
            type="button"
            onClick={() => onChange(opt.value)}
            className={
              "h-6 rounded-full px-3 font-medium transition-colors " +
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

type ToggleProps = {
  checked: boolean;
  onChange: (next: boolean) => void;
  label: string;
};

function Toggle({ checked, onChange, label }: ToggleProps) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      onClick={() => onChange(!checked)}
      className={
        "relative h-5 w-9 rounded-full transition-colors " +
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

type SettingsModalProps = {
  open: boolean;
  onClose: () => void;
};

export default function SettingsModal({
  open,
  onClose,
}: SettingsModalProps) {
  const [activeTab, setActiveTab] = useState<SettingsTab>("providers");

  useEffect(() => {
    if (!open) return;

    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };

    document.addEventListener("keydown", onKey);

    return () => document.removeEventListener("keydown", onKey);
  }, [open, onClose]);

  if (!open) return null;

  return (
    <div
      className={
        "fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-4"
      }
      onClick={onClose}
      role="dialog"
      aria-modal="true"
      aria-label="Settings"
    >
      <div
        className={
          "flex h-[68vh] max-h-[620px] w-full max-w-5xl overflow-hidden " +
          "rounded-2xl border border-border-primary bg-bg-secondary shadow-4xl"
        }
        onClick={(e) => e.stopPropagation()}
      >
        <SettingsSidebar activeTab={activeTab} onTabChange={setActiveTab} />
        <div className="min-h-0 flex-1 overflow-y-auto px-4 py-3">
          {activeTab === "models" && <ModelsPage onNavigate={setActiveTab} />}
          {activeTab === "providers" && <ProvidersPage />}
        </div>
      </div>
    </div>
  );
}

export { SettingsCard, Segmented, Toggle };
