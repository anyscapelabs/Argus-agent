import { useEffect } from "react";

import AgentsPage from "./AgentsPage";
import ModelsPage from "./ModelsPage";
import ProfilesPage from "./ProfilesPage";
import ProvidersPage from "./ProvidersPage";
import SandboxPage from "./SandboxPage";
import SettingsSidebar, { type SettingsTab } from "./SettingsSidebar";
import TerminalPage from "./TerminalPage";
import ThemePage from "./ThemePage";

const TITLES: Record<SettingsTab, string> = {
  models: "Models",
  providers: "Providers",
  terminal: "Terminal",
  sandbox: "Sandbox",
  agents: "Sub-agents",
  profiles: "Profiles",
  theme: "Appearance",
};

type Props = {
  tab: SettingsTab;
  onTab: (tab: SettingsTab) => void;
  onClose: () => void;
};

export default function SettingsPage({ tab, onTab, onClose }: Props) {
  useEffect(() => {
    const onKey = (evt: KeyboardEvent) => {
      if (evt.key === "Escape") {
        onClose();
      }
    };

    document.addEventListener("keydown", onKey);

    return () => document.removeEventListener("keydown", onKey);
  }, [onClose]);

  return (
    <div className="flex h-full w-full overflow-hidden">
      <SettingsSidebar activeTab={tab} onTabChange={onTab} />
      <div className="min-h-0 min-w-0 flex-1 overflow-y-auto">
        <div className="mx-auto w-full max-w-3xl px-6 py-6">
          <h1 className="mb-4 text-lg font-semibold text-text-primary">
            {TITLES[tab]}
          </h1>
          {tab === "models" && <ModelsPage onNavigate={onTab} />}
          {tab === "providers" && <ProvidersPage />}
          {tab === "terminal" && <TerminalPage />}
          {tab === "sandbox" && <SandboxPage />}
          {tab === "agents" && <AgentsPage />}
          {tab === "profiles" && <ProfilesPage />}
          {tab === "theme" && <ThemePage />}
        </div>
      </div>
    </div>
  );
}
