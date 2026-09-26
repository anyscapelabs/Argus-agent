import { IoSparklesOutline } from "react-icons/io5";
import {
  LuPalette,
  LuServer,
  LuShield,
  LuTerminal,
  LuUsers,
} from "react-icons/lu";
import { TbUser } from "react-icons/tb";

export type SettingsTab =
  | "models"
  | "providers"
  | "terminal"
  | "sandbox"
  | "agents"
  | "profiles"
  | "theme";

type TabIcon = React.ComponentType<{ size?: number; className?: string }>;

const TABS: { tab: SettingsTab; label: string; Icon: TabIcon }[] = [
  { tab: "models", label: "Models", Icon: IoSparklesOutline },
  { tab: "providers", label: "Providers", Icon: LuServer },
  { tab: "terminal", label: "Terminal", Icon: LuTerminal },
  { tab: "sandbox", label: "Sandbox", Icon: LuShield },
  { tab: "agents", label: "Sub-agents", Icon: LuUsers },
  { tab: "profiles", label: "Profiles", Icon: TbUser },
  { tab: "theme", label: "Appearance", Icon: LuPalette },
];

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
      className={
        "flex w-60 shrink-0 flex-col border-r border-border-primary " +
        "bg-bg-secondary"
      }
      aria-label="Settings sections"
    >
      <h2 className="px-4 pb-1 pt-4 text-sm font-semibold text-text-primary">
        Settings
      </h2>
      <nav className="flex flex-col gap-0.5 p-2">
        {TABS.map(({ tab, label, Icon }) => {
          const active = activeTab === tab;

          return (
            <button
              key={tab}
              type="button"
              onClick={() => onTabChange(tab)}
              aria-current={active ? "page" : undefined}
              className={
                "flex items-center gap-2 rounded-md px-2 py-1 text-sm " +
                "font-medium transition-colors " +
                `${
                  active
                    ? "bg-bg-hover-secondary text-text-primary"
                    : "text-text-secondary hover:bg-bg-hover-secondary " +
                      "hover:text-text-primary"
                }`
              }
            >
              <Icon
                size={16}
                className={
                  active ? "text-text-primary" : "text-text-secondary"
                }
              />
              <span>{label}</span>
            </button>
          );
        })}
      </nav>
    </aside>
  );
}
