import { useEffect, useState } from "react";

import { profileStore, profileLabel, useProfiles } from "../../stores/profiles";
import { toast } from "../../stores/toast";

const DEFAULT_ID = "default";
const NAME_MAX = 40;

const INPUT =
  "w-full rounded-lg border border-border-primary bg-bg-primary px-3 py-2 " +
  "text-sm text-text-primary outline-none placeholder:text-text-tertiary " +
  "focus:border-accent";

function ProfileRow({ id }: { id: string }) {
  const { profiles, activeId } = useProfiles();
  const p = profiles.find((x) => x.id === id);

  const [name, setName] = useState(p?.name ?? "");
  const [body, setBody] = useState(p?.instructions ?? "");

  useEffect(() => {
    setName(p?.name ?? "");
    setBody(p?.instructions ?? "");
  }, [p?.name, p?.instructions]);

  if (p === undefined) {
    return null;
  }

  const isDefault = p.id === DEFAULT_ID;
  const dirty = name !== p.name || body !== p.instructions;
  const on = p.id === activeId;

  const save = async (patch: { name?: string; instructions?: string }) => {
    try {
      await profileStore.edit(p.id, patch);
    } catch (err) {
      toast.error(err instanceof Error ? err.message : String(err));
      setName(p.name);
      setBody(p.instructions);
    }
  };

  const remove = async () => {
    try {
      await profileStore.remove(p.id);
      toast.success(`Deleted ${profileLabel(p)}`);
    } catch (err) {
      toast.error(err instanceof Error ? err.message : String(err));
    }
  };

  return (
    <div className="rounded-xl border border-border-primary p-4">
      <div className="mb-3 flex items-center gap-2">
        <h3 className="min-w-0 flex-1 truncate text-sm font-medium text-text-primary">
          {profileLabel(p)}
        </h3>
        {on && (
          <span className="rounded-full bg-bg-hover-secondary px-2 py-0.5 text-xs text-text-secondary">
            In use
          </span>
        )}
        {!isDefault && (
          <button
            type="button"
            onClick={() => void remove()}
            className="text-xs text-text-secondary transition-colors hover:text-red-400"
          >
            Delete
          </button>
        )}
      </div>

      <div className="flex flex-col gap-3">
        <div>
          <label
            htmlFor={`name-${p.id}`}
            className="mb-1 block text-xs text-text-secondary"
          >
            Name
          </label>
          <input
            id={`name-${p.id}`}
            value={name}
            maxLength={NAME_MAX}
            disabled={isDefault}
            placeholder="Default"
            onChange={(e) => setName(e.target.value)}
            onBlur={() => {
              if (name !== p.name) {
                void save({ name });
              }
            }}
            onKeyDown={(e) => {
              if (e.key === "Enter" && name !== p.name) {
                void save({ name });
              }
            }}
            className={INPUT + " disabled:opacity-60"}
          />
        </div>

        <div>
          <label
            htmlFor={`body-${p.id}`}
            className="mb-1 block text-xs text-text-secondary"
          >
            Instructions
          </label>
          <textarea
            id={`body-${p.id}`}
            value={body}
            rows={5}
            placeholder={
              "What this profile produces, and what 'done' means to it. " +
              "A name on its own changes nothing."
            }
            onChange={(e) => setBody(e.target.value)}
            className={INPUT + " resize-y font-mono text-xs leading-relaxed"}
          />
          <p className="mt-1 text-xs text-text-tertiary">
            Added to the base prompt, never a replacement for it — the safety
            and tool rules stay whatever you write here. Takes effect the next
            time a chat starts a turn.
          </p>
        </div>

        {dirty && (
          <div className="flex justify-end">
            <button
              type="button"
              onClick={() => void save({ name, instructions: body })}
              className={
                "rounded-lg bg-accent px-3 py-1.5 text-xs font-medium " +
                "text-white transition-opacity hover:opacity-90"
              }
            >
              Save
            </button>
          </div>
        )}
      </div>
    </div>
  );
}

export default function ProfilesPage() {
  const { profiles, loading } = useProfiles();

  useEffect(() => {
    void profileStore.load();
  }, []);

  if (loading) {
    return null;
  }

  return (
    <div className="flex flex-col gap-3">
      <p className="text-xs text-text-secondary">
        A profile is a name and a set of instructions. It owns its chats and
        the sub-agents inside them, and nothing else — providers, models and
        appearance are yours, not its.
      </p>
      {profiles.map((p) => (
        <ProfileRow key={p.id} id={p.id} />
      ))}
    </div>
  );
}
