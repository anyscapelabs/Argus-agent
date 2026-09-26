import { useEffect, useState } from "react";
import { FiPlus } from "react-icons/fi";
import { LuChevronRight } from "react-icons/lu";

import type { ProfileRow } from "../../lib/ipc";
import { profileStore, profileLabel, useProfiles } from "../../stores/profiles";
import { Card, Page, ProfileAvatar, Section } from "./kit";
import ProfileDetail from "./ProfileDetail";

const ROW =
  "flex w-full items-center gap-4 px-4 py-3.5 text-left " +
  "transition-colors hover:bg-bg-hover-secondary";

/// The one line that tells two profiles apart. Instructions live on the detail
/// page; what you want here is how far this one reaches.
function reach(p: ProfileRow): string {
  if (p.reach_all) return "Reaches every profile";

  if (p.grants > 0) {
    return `Reaches ${p.grants} profile${p.grants === 1 ? "" : "s"}`;
  }

  return "Reaches nothing yet";
}

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
      <Section label="Profiles">
        <Card>
          {profiles.map((p) => (
            <button
              key={p.id}
              type="button"
              onClick={() => setOpenId(p.id)}
              className={ROW}
            >
              <ProfileAvatar name={p.name} size={32} />
              <span className="min-w-0 flex-1">
                <span className="block truncate text-sm text-text-primary">
                  {profileLabel(p)}
                </span>
                <span className="mt-0.5 block truncate text-xs text-text-tertiary">
                  {reach(p)}
                </span>
              </span>
              {p.id === activeId && (
                <span className="shrink-0 text-xs text-text-secondary">
                  Current
                </span>
              )}
              <LuChevronRight
                size={14}
                className="shrink-0 text-text-tertiary"
              />
            </button>
          ))}

          <button
            type="button"
            onClick={() => void profileStore.create("New profile")}
            disabled={profiles.length >= 10}
            className={
              ROW +
              " text-text-secondary disabled:cursor-not-allowed " +
              "disabled:opacity-40 disabled:hover:bg-transparent"
            }
          >
            <span className="flex h-8 w-8 shrink-0 items-center justify-center rounded-full border border-border-primary">
              <FiPlus size={13} />
            </span>
            <span className="min-w-0 flex-1 truncate text-sm">
              New profile
            </span>
          </button>
        </Card>
      </Section>
    </Page>
  );
}
