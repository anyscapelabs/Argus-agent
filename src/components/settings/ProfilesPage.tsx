import { useEffect, useState } from "react";
import { FiPlus } from "react-icons/fi";

import { profileStore, profileLabel, useProfiles } from "../../stores/profiles";
import { Page, ProfileAvatar } from "./kit";
import ProfileDetail from "./ProfileDetail";

const ADD =
  "flex flex-col items-center justify-center gap-3 rounded-xl border " +
  "border-dashed border-border-primary px-4 py-6 text-sm text-" +
  "text-secondary transition-colors hover:border-text-tertiary " +
  "hover:text-text-primary disabled:cursor-not-allowed disabled:opacity-40";

const TILE =
  "flex flex-col items-center gap-3 rounded-xl border border-border-primary " +
  "bg-bg-secondary px-4 py-6 transition-colors hover:bg-bg-hover-secondary";

export default function ProfilesPage() {
  const { profiles, loading, activeId } = useProfiles();
  const [openId, setOpenId] = useState<string | null>(null);

  useEffect(() => {
    void profileStore.load();
  }, []);

  const selected = profiles.find((p) => p.id === openId);

  if (selected !== undefined) {
    return (
      <ProfileDetail
        key={selected.id}
        id={selected.id}
        onBack={() => setOpenId(null)}
      />
    );
  }

  if (loading) {
    return null;
  }

  return (
    <Page>
      <div className="grid grid-cols-3 gap-3">
        {profiles.map((p) => (
          <button
            key={p.id}
            type="button"
            onClick={() => setOpenId(p.id)}
            className={TILE}
          >
            <span className="relative">
              <ProfileAvatar name={p.name} size={44} />
              {p.id === activeId && (
                <span className="absolute -right-0.5 -bottom-0.5 h-3 w-3 rounded-full border-2 border-bg-secondary bg-accent" />
              )}
            </span>
            <span className="max-w-full truncate text-sm text-text-primary">
              {profileLabel(p)}
            </span>
            <span className="line-clamp-2 min-h-8 text-center text-xs leading-snug text-text-tertiary">
              {p.instructions.trim() === ""
                ? "No instructions yet"
                : p.instructions.trim().replace(/\s+/g, " ")}
            </span>
          </button>
        ))}

        <button
          type="button"
          onClick={() => void profileStore.create("New profile")}
          disabled={profiles.length >= 10}
          className={ADD}
        >
          <FiPlus size={18} />
          <span>New profile</span>
        </button>
      </div>
    </Page>
  );
}
