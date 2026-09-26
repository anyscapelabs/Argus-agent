import { useEffect, useState } from "react";
import { FiArrowLeft } from "react-icons/fi";

import { profileLabel, useProfiles } from "../../stores/profiles";
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

export default function SettingsPage({ tab, onTab, onClose }: Props) {
  const [openId, setOpenId] = useState<string | null>(null);
  const { profiles } = useProfiles();

  useEffect(() => {
    setOpenId(null);
  }, [tab]);

  // One profile deep is a page of its own, so Escape walks back out of it before
  // it walks out of settings.
  useEffect(() => {
    const onKey = (evt: KeyboardEvent) => {
      if (evt.key !== "Escape") {
        return;
      }

      if (openId !== null) {
        setOpenId(null);
        return;
      }

      onClose();
    };

    document.addEventListener("keydown", onKey);

    return () => document.removeEventListener("keydown", onKey);
  }, [onClose, openId]);

  const open = openId === null ? undefined : profiles.find((p) => p.id === openId);

  return (
    <div className="h-full w-full overflow-y-auto">
      {/* The reach grid needs columns a 3xl column does not have. */}
      <div
        className={
          "mx-auto w-full px-8 py-8 " +
          (tab === "profiles" ? "max-w-5xl" : "max-w-3xl")
        }
      >
        {open !== undefined ? (
          <div className="mb-6 flex flex-col">
            <button
              type="button"
              onClick={() => setOpenId(null)}
              className={
                "mb-3 flex w-fit items-center gap-1.5 text-xs text-" +
                "text-secondary transition-colors hover:text-text-primary"
              }
            >
              <FiArrowLeft size={13} />
              <span>Profiles</span>
            </button>
            <h1 className="truncate text-[22px] font-normal leading-tight text-text-primary">
              {profileLabel(open)}
            </h1>
          </div>
        ) : (
          <div className="mb-6 flex flex-col">
            <h1 className="text-[22px] font-normal leading-tight text-text-primary">
              {SETTINGS_TITLE[tab]}
            </h1>
            <p className="mt-2 text-sm leading-relaxed text-text-secondary">
              {SETTINGS_DESC[tab]}
            </p>
          </div>
        )}

        {tab === "models" && <ModelsPage onNavigate={onTab} />}
        {tab === "providers" && <ProvidersPage />}
        {tab === "terminal" && <TerminalPage />}
        {tab === "sandbox" && <SandboxPage />}
        {tab === "profiles" && (
          <ProfilesPage openId={openId} onOpen={setOpenId} />
        )}
        {tab === "theme" && <ThemePage />}
      </div>
    </div>
  );
}
