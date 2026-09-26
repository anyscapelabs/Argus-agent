import { useEffect } from "react";

import AgentsPage from "./AgentsPage";
import ModelsPage from "./ModelsPage";
import ProfilesPage from "./ProfilesPage";
import ProvidersPage from "./ProvidersPage";
import SandboxPage from "./SandboxPage";
import { SETTINGS_DESC, SETTINGS_TITLE, type SettingsTab } from "./tabs";
import TerminalPage from "./TerminalPage";
import ThemePage from "./ThemePage";

type Props = {
  tab: SettingsTab;
  onTab: (tab: SettingsTab) => void;
  onClose: () => void;
};

export default function SettingsPage({
  tab,
  onTab,
  onClose,
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
          "mx-auto w-full px-8 py-10 " +
          (tab === "profiles" ? "max-w-5xl" : "max-w-3xl")
        }
      >
        <div className="mb-8 flex flex-col">
          <h1 className="text-[22px] font-normal leading-tight text-text-primary">
            {SETTINGS_TITLE[tab]}
          </h1>
          <p className="mt-2 text-sm leading-relaxed text-text-secondary">
            {SETTINGS_DESC[tab]}
          </p>
        </div>

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
