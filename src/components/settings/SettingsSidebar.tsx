import { IoSparklesOutline } from "react-icons/io5";
import { LuServer } from "react-icons/lu";

export type SettingsTab = "models" | "providers";

type SettingsSidebarProps = {
  activeTab: SettingsTab;
  onTabChange: (tab: SettingsTab) => void;
};

export default function SettingsSidebar({ activeTab, onTabChange }: SettingsSidebarProps) {
  return (
    <aside className="flex w-56 shrink-0 flex-col border-r border-border-primary bg-bg-secondary" aria-label="Settings sidebar">
      <nav className="flex flex-col gap-0.5 p-2">
        <button
          type="button"
          onClick={() => onTabChange("models")}
          aria-current={activeTab === "models" ? "page" : undefined}
          className={`flex items-center gap-2 rounded-md px-2 py-1 text-sm font-medium transition-colors ${
            activeTab === "models"
              ? "bg-bg-hover-secondary text-text-primary"
              : "text-text-secondary hover:bg-bg-hover-secondary hover:text-text-primary"
          }`}
        >
          <IoSparklesOutline size={16} className={activeTab === "models" ? "text-text-primary" : "text-text-secondary"} />
          <span>Models</span>
        </button>
        <button
          type="button"
          onClick={() => onTabChange("providers")}
          aria-current={activeTab === "providers" ? "page" : undefined}
          className={`flex items-center gap-2 rounded-md px-2 py-1 text-sm font-medium transition-colors ${
            activeTab === "providers"
              ? "bg-bg-hover-secondary text-text-primary"
              : "text-text-secondary hover:bg-bg-hover-secondary hover:text-text-primary"
          }`}
        >
          <LuServer size={16} className={activeTab === "providers" ? "text-text-primary" : "text-text-secondary"} />
          <span>Providers</span>
        </button>
      </nav>
    </aside>
  );
}
