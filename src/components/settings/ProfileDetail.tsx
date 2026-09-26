import { useEffect, useState } from "react";
import { FiArrowLeft } from "react-icons/fi";

import { profileStore, profileLabel, useProfiles } from "../../stores/profiles";
import { toast } from "../../stores/toast";
import { Btn, Card, INPUT, Note, Page, ProfileAvatar, Row, Section } from "./kit";
import ReachMatrix, { type GrantCell } from "./ReachMatrix";

const DEFAULT_ID = "default";
const NAME_MAX = 40;

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

type Props = {
  id: string;
  onBack: () => void;
};

export default function ProfileDetail({ id, onBack }: Props) {
  const { profiles, reach, reachFor } = useProfiles();

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

  return (
    <Page>
      <button
        type="button"
        onClick={onBack}
        className={
          "flex w-fit items-center gap-1.5 text-xs text-text-secondary " +
          "transition-colors hover:text-text-primary"
        }
      >
        <FiArrowLeft size={13} />
        <span>Profiles</span>
      </button>

      <div className="-mt-4 flex items-center gap-3">
        <ProfileAvatar name={p.name} size={40} />
        <h2 className="min-w-0 flex-1 truncate text-[22px] font-normal leading-tight text-text-primary">
          {profileLabel(p)}
        </h2>
        {!isDefault && (
          <Btn variant="danger" onClick={() => void remove()}>
            Delete
          </Btn>
        )}
      </div>

      <Section label="Name">
        <Card>
          <Row stacked title="What you call it" desc="Safe to change at any time.">
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
              className={INPUT}
            />
            {isDefault && (
              <Note>
                The default always exists, so it has no name to change.
              </Note>
            )}
          </Row>
        </Card>
      </Section>

      <Section
        label="Instructions"
        note="Added to the base prompt, never a replacement for it — the safety and tool rules stay whatever you write here. Takes effect the next time a chat starts a turn."
      >
        <Card>
          <Row stacked title="How it works">
            <textarea
              value={body}
              rows={8}
              placeholder={
                "What this profile produces, and what 'done' means to it. " +
                "A name on its own changes nothing."
              }
              onChange={(e) => setBody(e.target.value)}
              className={INPUT + " resize-y font-mono text-xs leading-relaxed"}
            />
          </Row>
        </Card>
      </Section>

      <Section
        label="Reach"
        note="What this profile is allowed to do to your other profiles. Nothing here is used yet — the tools that would act on it are not built."
      >
        <Card>
          <div className="p-4">
            <ReachMatrix
              profileId={p.id}
              reachAll={matrix.reachAll}
              grants={matrix.grants}
              onChange={(reachAll, grants) => setMatrix({ reachAll, grants })}
            />
          </div>
        </Card>
      </Section>

      {dirty && (
        <div className="flex items-center gap-3">
          <span className="min-w-0 flex-1">
            <Note>Unsaved changes</Note>
          </span>
          <Btn variant="primary" onClick={() => void save()}>
            Save
          </Btn>
        </div>
      )}
    </Page>
  );
}
