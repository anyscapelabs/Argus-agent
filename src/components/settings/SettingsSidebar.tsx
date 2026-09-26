import { IoSparklesOutline } from "react-icons/io5";
import { LuPalette, LuServer, LuShield, LuTerminal, LuUsers } from "react-icons/lu";

export type SettingsTab =
  "models" | "providers" | "terminal" | "sandbox" | "agents" | "theme";

type SettingsSidebarProps = {
  activeTab: SettingsTab;
  onTabChange: (tab: SettingsTab) => void;
};

export default function SettingsSidebar({
  activeTab,
  onTabChange,
}: SettingsSidebarProps) {
  return (
    <aside
      className="flex w-56 shrink-0 flex-col border-r border-border-primary bg-bg-secondary"
      aria-label="Settings sidebar"
    >
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
          <IoSparklesOutline
            size={16}
            className={
              activeTab === "models"
                ? "text-text-primary"
                : "text-text-secondary"
            }
          />
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
          <LuServer
            size={16}
            className={
              activeTab === "providers"
                ? "text-text-primary"
                : "text-text-secondary"
            }
          />
          <span>Providers</span>
        </button>

        <button
          type="button"
          onClick={() => onTabChange("terminal")}
          aria-current={activeTab === "terminal" ? "page" : undefined}
          className={`flex items-center gap-2 rounded-md px-2 py-1 text-sm font-medium transition-colors ${
            activeTab === "terminal"
              ? "bg-bg-hover-secondary text-text-primary"
              : "text-text-secondary hover:bg-bg-hover-secondary hover:text-text-primary"
          }`}
        >
          <LuTerminal
            size={16}
            className={
              activeTab === "terminal"
                ? "text-text-primary"
                : "text-text-secondary"
            }
          />
          <span>Terminal</span>
        </button>

        <button
          type="button"
          onClick={() => onTabChange("sandbox")}
          aria-current={activeTab === "sandbox" ? "page" : undefined}
          className={`flex items-center gap-2 rounded-md px-2 py-1 text-sm font-medium transition-colors ${
            activeTab === "sandbox"
              ? "bg-bg-hover-secondary text-text-primary"
              : "text-text-secondary hover:bg-bg-hover-secondary hover:text-text-primary"
          }`}
        >
          <LuShield
            size={16}
            className={
              activeTab === "sandbox"
                ? "text-text-primary"
                : "text-text-secondary"
            }
          />
          <span>Sandbox</span>
        </button>
        <button
          type="button"
          onClick={() => onTabChange("agents")}
          aria-current={activeTab === "agents" ? "page" : undefined}
          className={`flex items-center gap-2 rounded-md px-2 py-1 text-sm font-medium transition-colors ${
            activeTab === "agents"
              ? "bg-bg-hover-secondary text-text-primary"
              : "text-text-secondary hover:bg-bg-hover-secondary hover:text-text-primary"
          }`}
        >
          <LuUsers
            size={16}
            className={
              activeTab === "agents"
                ? "text-text-primary"
                : "text-text-secondary"
            }
          />
          <span>Sub-agents</span>
        </button>

        <button
          type="button"
          onClick={() => onTabChange("theme")}
          aria-current={activeTab === "theme" ? "page" : undefined}
          className={`flex items-center gap-2 rounded-md px-2 py-1 text-sm font-medium transition-colors ${
            activeTab === "theme"
              ? "bg-bg-hover-secondary text-text-primary"
              : "text-text-secondary hover:bg-bg-hover-secondary hover:text-text-primary"
          }`}
        >
          <LuPalette
            size={16}
            className={
              activeTab === "theme"
                ? "text-text-primary"
                : "text-text-secondary"
            }
          />
          <span>Appearance</span>
        </button>
      </nav>
    </aside>
  );
}
