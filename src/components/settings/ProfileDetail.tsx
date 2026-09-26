import { useEffect, useState, type ReactNode } from "react";
import { FiArrowLeft, FiMessageSquare } from "react-icons/fi";

import { profileStore, profileLabel, useProfiles } from "../../stores/profiles";
import { useSessions } from "../../stores/sessions";
import { toast } from "../../stores/toast";
import { ProfileAvatar } from "./kit";
import ReachMatrix, { type GrantCell } from "./ReachMatrix";

const DEFAULT_ID = "default";
const NAME_MAX = 40;

const INPUT =
  "w-full rounded-lg border border-border-primary bg-bg-primary px-3 py-2 " +
  "text-sm text-text-primary outline-none placeholder:text-text-tertiary " +
  "focus:border-accent";

/// A matrix is a set of cells, so two of them are the same matrix however the
/// user got there. Comparing the arrays directly would call a reorder a change
/// and leave a Save button on screen with nothing behind it.
function sameGrants(a: GrantCell[], b: GrantCell[]): boolean {
  return (
    a.length === b.length &&
    a.every((g) =>
      b.some(
        (h) => h.capability === g.capability && h.target_id === g.target_id,
      ),
    )
  );
}

function Section({
  title,
  hint,
  children,
}: {
  title: string;
  hint: string;
  children: ReactNode;
}) {
  return (
    <section className="border-t border-border-primary py-5 first:border-t-0">
      <div className="mb-3">
        <h2 className="text-sm font-medium text-text-primary">{title}</h2>
        <p className="text-xs text-text-secondary">{hint}</p>
      </div>
      {children}
    </section>
  );
}

type Props = {
  id: string;
  onBack: () => void;
  onOpenSession: (id: string) => void;
};

export default function ProfileDetail({ id, onBack, onOpenSession }: Props) {
  const { profiles, reach, reachFor } = useProfiles();
  const { sessions } = useSessions();

  const p = profiles.find((x) => x.id === id);

  const [name, setName] = useState(p?.name ?? "");
  const [body, setBody] = useState(p?.instructions ?? "");
  const [matrix, setMatrix] = useState<{
    reachAll: boolean;
    grants: GrantCell[];
  }>({ reachAll: false, grants: [] });

  useEffect(() => {
    void profileStore.loadReach(id);
  }, [id]);

  // Seed the staged copy from the store once its answer lands. `reachFor` is
  // the guard: a slow read for a profile you have already navigated away from
  // must not repopulate the form you are looking at now.
  useEffect(() => {
    if (reachFor !== id) {
      return;
    }

    setMatrix({ reachAll: reach.reach_all, grants: reach.grants });
  }, [reachFor, reach, id]);

  useEffect(() => {
    setName(p?.name ?? "");
    setBody(p?.instructions ?? "");
  }, [p?.name, p?.instructions]);

  if (p === undefined) {
    return null;
  }

  const isDefault = p.id === DEFAULT_ID;
  // Until this profile's own matrix has landed, `reach` holds whatever was
  // loaded last and comparing against it would invent a diff.
  const loaded = reachFor === id;
  const reachDirty =
    loaded &&
    (matrix.reachAll !== p.reach_all || !sameGrants(matrix.grants, reach.grants));
  const dirty = body !== p.instructions || reachDirty;

  const save = async () => {
    try {
      if (body !== p.instructions) {
        await profileStore.edit(p.id, { instructions: body });
      }

      if (reachDirty) {
        await profileStore.saveReach(p.id, matrix.reachAll, matrix.grants);
      }

      toast.success("Saved");
    } catch (err) {
      toast.error(err instanceof Error ? err.message : String(err));
    }
  };

  const remove = async () => {
    try {
      await profileStore.remove(p.id);
      toast.success(`Deleted ${profileLabel(p)}`);
      onBack();
    } catch (err) {
      toast.error(err instanceof Error ? err.message : String(err));
    }
  };

  const chats = sessions.filter((s) => (s.profile_id ?? DEFAULT_ID) === p.id);

  return (
    <div className="flex flex-col">
      <button
        type="button"
        onClick={onBack}
        className={
          "mb-4 flex w-fit items-center gap-1.5 text-xs text-text-secondary " +
          "transition-colors hover:text-text-primary"
        }
      >
        <FiArrowLeft size={13} />
        <span>Profiles</span>
      </button>

      <div className="mb-5 flex items-center gap-3">
        <ProfileAvatar name={p.name} size={36} />
        <h1 className="min-w-0 flex-1 truncate text-lg font-semibold text-text-primary">
          {profileLabel(p)}
        </h1>
        {!isDefault && (
          <button
            type="button"
            onClick={() => void remove()}
            className="shrink-0 text-xs text-text-secondary transition-colors hover:text-red-400"
          >
            Delete profile
          </button>
        )}
      </div>

      <Section
        title="Name"
        hint="What you call it in the picker. Safe to change at any time."
      >
        <input
          value={name}
          maxLength={NAME_MAX}
          disabled={isDefault}
          placeholder="Default"
          onChange={(e) => setName(e.target.value)}
          onBlur={() => {
            if (name !== p.name) {
              void profileStore.edit(p.id, { name });
            }
          }}
          onKeyDown={(e) => {
            if (e.key === "Enter" && name !== p.name) {
              void profileStore.edit(p.id, { name });
            }
          }}
          className={INPUT + " disabled:opacity-60"}
        />
        {isDefault && (
          <p className="mt-1.5 text-xs text-text-tertiary">
            The default always exists, so it has no name to change.
          </p>
        )}
      </Section>

      <Section
        title="Instructions"
        hint="Added to the base prompt, never a replacement for it — the safety and tool rules stay whatever you write here."
      >
        <textarea
          value={body}
          rows={7}
          placeholder={
            "What this profile produces, and what 'done' means to it. " +
            "A name on its own changes nothing."
          }
          onChange={(e) => setBody(e.target.value)}
          className={INPUT + " resize-y font-mono text-xs leading-relaxed"}
        />
        <p className="mt-1.5 text-xs text-text-tertiary">
          Takes effect the next time a chat starts a turn. The chats below keep
          their history; only the answering changes.
        </p>
      </Section>

      <Section
        title="Reach"
        hint="What this profile is allowed to do to your other profiles. Nothing here is used yet — the tools that would act on it are not built."
      >
        <ReachMatrix
          profileId={p.id}
          reachAll={matrix.reachAll}
          grants={matrix.grants}
          onChange={(reachAll, grants) => setMatrix({ reachAll, grants })}
        />
      </Section>

      <Section
        title={chats.length === 1 ? "1 chat" : `${chats.length} chats`}
        hint="Chats this profile owns. A profile that still has any cannot be deleted."
      >
        {chats.length === 0 ? (
          <p className="text-xs text-text-tertiary">
            No chats yet. New chats land here once this profile is the one you
            are working in.
          </p>
        ) : (
          <ul className="overflow-hidden rounded-xl border border-border-primary">
            {chats.map((s) => (
              <li key={s.id} className="border-b border-border-primary last:border-b-0">
                <button
                  type="button"
                  onClick={() => onOpenSession(s.id)}
                  className="flex w-full items-center gap-2.5 px-3 py-2 text-left transition-colors hover:bg-bg-hover-primary"
                >
                  <FiMessageSquare
                    size={13}
                    className="shrink-0 text-text-tertiary"
                  />
                  <span className="min-w-0 flex-1 truncate text-sm text-text-primary">
                    {s.title}
                  </span>
                </button>
              </li>
            ))}
          </ul>
        )}
      </Section>

      {dirty && (
        <div className="sticky bottom-0 -mx-6 flex items-center gap-3 border-t border-border-primary bg-bg-primary px-6 py-3">
          <span className="min-w-0 flex-1 text-xs text-text-secondary">
            Unsaved changes
          </span>
          <button
            type="button"
            onClick={() => void save()}
            className={
              "shrink-0 rounded-lg bg-accent px-3 py-1.5 text-xs font-medium " +
              "text-white transition-opacity hover:opacity-90"
            }
          >
            Save
          </button>
        </div>
      )}
    </div>
  );
}
