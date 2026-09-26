import { useEffect } from "react";

import AgentsPage from "./AgentsPage";
import ModelsPage from "./ModelsPage";
import ProfilesPage from "./ProfilesPage";
import ProvidersPage from "./ProvidersPage";
import SandboxPage from "./SandboxPage";
import { SETTINGS_TITLE, type SettingsTab } from "./tabs";
import TerminalPage from "./TerminalPage";
import ThemePage from "./ThemePage";

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
    <div className="h-full w-full overflow-y-auto">
      <div className="mx-auto w-full max-w-3xl px-6 py-6">
        <h1 className="mb-4 text-lg font-semibold text-text-primary">
          {SETTINGS_TITLE[tab]}
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
  );
}
