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
  onOpenSession: (id: string) => void;
};

export default function SettingsPage({
  tab,
  onTab,
  onClose,
  onOpenSession,
}: Props) {
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
      {/* The reach grid needs columns a 3xl column does not have. */}
      <div
        className={
          "mx-auto w-full px-6 py-6 " +
          (tab === "profiles" ? "max-w-5xl" : "max-w-3xl")
        }
      >
        {tab !== "profiles" && (
          <h1 className="mb-4 text-lg font-semibold text-text-primary">
            {SETTINGS_TITLE[tab]}
          </h1>
        )}
        {tab === "models" && <ModelsPage onNavigate={onTab} />}
        {tab === "providers" && <ProvidersPage />}
        {tab === "terminal" && <TerminalPage />}
        {tab === "sandbox" && <SandboxPage />}
        {tab === "agents" && <AgentsPage />}
        {tab === "profiles" && <ProfilesPage onOpenSession={onOpenSession} />}
        {tab === "theme" && <ThemePage />}
      </div>
    </div>
  );
}
