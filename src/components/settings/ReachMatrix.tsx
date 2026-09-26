import { FiCheck } from "react-icons/fi";

import { CAPABILITIES, profileLabel, useProfiles } from "../../stores/profiles";
import { Note, Segmented } from "./kit";

export type GrantCell = { capability: string; target_id: string };

type Props = {
  profileId: string;
  reachAll: boolean;
  grants: GrantCell[];
  onChange: (reachAll: boolean, grants: GrantCell[]) => void;
};

export default function ReachMatrix({
  profileId,
  reachAll,
  grants,
  onChange,
}: Props) {
  const { profiles } = useProfiles();

  // A profile is never a column of its own matrix. The backend refuses the
  // cell, so offering it here would be a switch that always springs back.
  const targets = profiles.filter((p) => p.id !== profileId);

  const on = (capability: string, targetId: string) =>
    grants.some((g) => g.capability === capability && g.target_id === targetId);

  const flip = (capability: string, targetId: string) => {
    const kept = grants.filter(
      (g) => !(g.capability === capability && g.target_id === targetId),
    );

    onChange(
      reachAll,
      on(capability, targetId) ? kept : [...kept, { capability, target_id: targetId }],
    );
  };

  // The matrix is kept, not cleared. "All" and "some" are a dial between two
  // saved settings, not a thing you do and undo.
  const setMode = (all: boolean) => onChange(all, grants);

  if (targets.length === 0) {
    return (
      <p className="rounded-lg bg-bg-secondary px-3 py-2.5 text-xs text-text-tertiary">
        There is no other profile to reach yet. Create one and this becomes a
        grid of what this profile may do to it.
      </p>
    );
  }

  return (
    <div className="flex flex-col gap-4">
      <div className="flex items-center gap-3">
        <div className="min-w-0 flex-1">
          <h3 className="text-sm font-medium text-text-primary">
            Can reach other profiles
          </h3>
          <p className="text-xs text-text-secondary">
            {reachAll
              ? "Every profile, including ones you create later."
              : "Off is the default, and a profile that reaches nothing is not missing a feature."}
          </p>
        </div>
        <Segmented
          value={reachAll ? "all" : "specific"}
          opts={[
            { value: "specific", label: "Specific" },
            { value: "all", label: "All" },
          ]}
          onChange={(v) => setMode(v === "all")}
        />
      </div>

      {reachAll ? (
        <Note>
          All {targets.length} of your other profiles, with every capability,
          including ones you create later. Switch back to Specific to pick them
          one at a time — the picks you had are kept.
        </Note>
      ) : (
        <div className="overflow-x-auto">
          <p className="mb-3 text-xs text-text-tertiary">
            Tick a cell to let this profile do that to that one. Each row hands
            over more than the row above it, and nothing is shared between
            columns.
          </p>
          <table className="w-full border-separate border-spacing-0">
            <thead>
              <tr>
                <th className="w-60 pb-2 text-left text-xs font-normal text-text-tertiary">
                  Capability
                </th>
                {targets.map((t) => (
                  <th
                    key={t.id}
                    className="w-16 px-1 pb-2 text-center text-xs font-normal text-text-secondary"
                  >
                    <span className="block truncate">{profileLabel(t)}</span>
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {CAPABILITIES.map((c, i) => (
                <tr
                  key={c.key}
                  className={i > 0 ? "border-t border-border-primary" : ""}
                >
                  <th className="py-2.5 pr-3 text-left align-top font-normal">
                    <span className="block text-sm text-text-primary">
                      {c.label}
                    </span>
                    <span className="block text-xs leading-snug text-text-tertiary">
                      {c.note}
                    </span>
                  </th>
                  {targets.map((t) => {
                    const lit = on(c.key, t.id);

                    return (
                      <td key={t.id} className="px-1 py-2.5 text-center align-top">
                        <button
                          type="button"
                          aria-label={`${c.label} — ${profileLabel(t)}`}
                          aria-pressed={lit}
                          onClick={() => flip(c.key, t.id)}
                          className={
                            "mx-auto flex h-5 w-5 items-center justify-center " +
                            "rounded border transition-colors " +
                            (lit
                              ? "border-accent bg-accent text-white"
                              : "border-border-primary hover:border-text-tertiary")
                          }
                        >
                          {lit && <FiCheck size={12} />}
                        </button>
                      </td>
                    );
                  })}
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}
