import { useEffect, useMemo, useState } from "react";
import { FiChevronRight, FiPlus } from "react-icons/fi";
import { LuLock } from "react-icons/lu";

import { profileStore, profileLabel, useProfiles } from "../../stores/profiles";
import { useSessions } from "../../stores/sessions";
import { ProfileAvatar } from "./kit";
import ProfileDetail from "./ProfileDetail";

const DEFAULT_ID = "default";

const ADD =
  "flex w-full items-center gap-3 px-3 py-2.5 text-left text-sm text-text-secondary " +
  "transition-colors hover:bg-bg-hover-primary hover:text-text-primary " +
  "disabled:cursor-not-allowed disabled:opacity-50 disabled:hover:bg-transparent";

export default function ProfilesPage({
  onOpenSession,
}: {
  onOpenSession: (id: string) => void;
}) {
  const { profiles, loading, activeId } = useProfiles();
  const { sessions } = useSessions();
  const [openId, setOpenId] = useState<string | null>(null);

  useEffect(() => {
    void profileStore.load();
  }, []);

  // Chats are counted here rather than asked for, because the list already
  // holds every session in memory and a per-profile count query would be a
  // round trip to learn something the data in front of us already says.
  const owned = useMemo(() => {
    const n = new Map<string, number>();

    for (const s of sessions) {
      const id = s.profile_id ?? DEFAULT_ID;
      n.set(id, (n.get(id) ?? 0) + 1);
    }

    return n;
  }, [sessions]);

  const selected = profiles.find((p) => p.id === openId);

  if (selected !== undefined) {
    return (
      <ProfileDetail
        key={selected.id}
        id={selected.id}
        onBack={() => setOpenId(null)}
        onOpenSession={onOpenSession}
      />
    );
  }

  if (loading) {
    return null;
  }

  return (
    <div className="flex flex-col">
      <h1 className="mb-1 text-lg font-semibold text-text-primary">Profiles</h1>
      <p className="mb-5 text-xs text-text-secondary">
        A profile is a name, a set of instructions, and what it may reach. It
        owns its chats and the sub-agents inside them, and nothing else —
        providers, models and appearance are yours, not its.
      </p>

      <div className="overflow-hidden rounded-xl border border-border-primary">
        {profiles.map((p) => {
          const count = owned.get(p.id) ?? 0;
          const canReach = p.reach_all || p.grants > 0;
          const blank = p.instructions.trim() === "";

          return (
            <button
              key={p.id}
              type="button"
              onClick={() => setOpenId(p.id)}
              className={
                "flex w-full items-center gap-3 border-b border-border-primary " +
                "px-3 py-2.5 text-left transition-colors last:border-b-0 " +
                "hover:bg-bg-hover-primary"
              }
            >
              <ProfileAvatar name={p.name} />
              <span className="min-w-0 flex-1">
                <span className="flex items-center gap-2">
                  <span className="truncate text-sm font-medium text-text-primary">
                    {profileLabel(p)}
                  </span>
                  {p.id === activeId && (
                    <span className="h-1.5 w-1.5 shrink-0 rounded-full bg-accent" />
                  )}
                </span>
                <span className="mt-0.5 block truncate text-xs text-text-tertiary">
                  {blank
                    ? "No instructions yet — a name on its own changes nothing"
                    : p.instructions.trim().replace(/\s+/g, " ")}
                </span>
              </span>

              <span className="flex shrink-0 items-center gap-2.5 text-xs text-text-tertiary">
                {canReach && (
                  <span title="Reaches other profiles">
                    <LuLock size={13} />
                  </span>
                )}
                <span>
                  {count} {count === 1 ? "chat" : "chats"}
                </span>
              </span>

              <FiChevronRight
                size={15}
                className="shrink-0 text-text-tertiary"
              />
            </button>
          );
        })}

        <button
          type="button"
          onClick={() => void profileStore.create("New profile")}
          disabled={profiles.length >= 10}
          className={ADD}
        >
          <FiPlus size={16} className="shrink-0" />
          <span>New profile</span>
        </button>
      </div>
    </div>
  );
}
