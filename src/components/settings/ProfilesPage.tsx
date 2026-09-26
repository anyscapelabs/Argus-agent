import { useEffect, useState } from "react";
import { FiChevronRight, FiPlus } from "react-icons/fi";

import { profileStore, profileLabel, useProfiles } from "../../stores/profiles";
import { Card, Page, ProfileAvatar } from "./kit";
import ProfileDetail from "./ProfileDetail";

const ADD =
  "flex w-full items-center gap-3 px-4 py-3.5 text-sm text-text-secondary " +
  "transition-colors hover:bg-bg-hover-secondary hover:text-text-primary " +
  "disabled:cursor-not-allowed disabled:opacity-50 disabled:hover:bg-bg-secondary";

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
      <Card>
        {profiles.map((p) => (
          <button
            key={p.id}
            type="button"
            onClick={() => setOpenId(p.id)}
            className={
              "flex w-full items-center gap-3 px-4 py-3.5 text-left " +
              "transition-colors hover:bg-bg-hover-secondary"
            }
          >
            <ProfileAvatar name={p.name} />
            <span className="min-w-0 flex-1">
              <span className="flex items-center gap-2">
                <span className="truncate text-sm text-text-primary">
                  {profileLabel(p)}
                </span>
                {p.id === activeId && (
                  <span className="h-1.5 w-1.5 shrink-0 rounded-full bg-accent" />
                )}
              </span>
              <span className="mt-0.5 block truncate text-xs text-text-tertiary">
                {p.instructions.trim() === ""
                  ? "No instructions yet"
                  : p.instructions.trim().replace(/\s+/g, " ")}
              </span>
            </span>
            <FiChevronRight
              size={15}
              className="shrink-0 text-text-tertiary"
            />
          </button>
        ))}

        <button
          type="button"
          onClick={() => void profileStore.create("New profile")}
          disabled={profiles.length >= 10}
          className={ADD}
        >
          <FiPlus size={16} className="shrink-0" />
          <span>New profile</span>
        </button>
      </Card>
    </Page>
  );
}
