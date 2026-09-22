import { useEffect, useState } from "react";
import { LuArrowLeft } from "react-icons/lu";

import {
  skillCreate,
  skillUpdate,
  type Skill,
} from "../lib/ipc";

type Props = {
  skill: Skill | null;
  onBack: () => void;
  onSaved: (skill: Skill) => void;
};

const NAME_RE = /^[a-z0-9]+(-[a-z0-9]+)*$/;

export default function SkillEditorPage({
  skill,
  onBack,
  onSaved,
}: Props) {
  const editing = skill !== null;
  const [name, setName] = useState(skill?.name ?? "");
  const [desc, setDesc] = useState(skill?.description ?? "");
  const [body, setBody] = useState(skill?.body ?? "");
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);

  useEffect(() => {
    if (editing || skill !== null) return;

    setName("");
    setDesc("");
    setBody("");
    setErr(null);
  }, [editing, skill]);

  const nameBad = !editing && !NAME_RE.test(name.trim());

  const save = async () => {
    if (busy) return;

    setBusy(true);
    setErr(null);

    try {
      const out = editing
        ? await skillUpdate(skill.name, {
            description: desc.trim(),
            body,
          })
        : await skillCreate({
            name: name.trim(),
            description: desc.trim(),
            body,
            source: "user",
          });

      onSaved(out);
    } catch (e) {
      setErr(String(e));
      setBusy(false);
    }
  };

  return (
    <div className="py-4">
      <button
        type="button"
        onClick={onBack}
        className="flex items-center gap-1.5 text-sm text-text-secondary hover:text-text-primary cursor-pointer"
      >
        <LuArrowLeft size={15} />
        {editing ? skill.name : "Skills"}
      </button>

      <h2 className="mt-6 text-xl font-medium text-text-primary">
        {editing ? "Edit skill" : "New skill"}
      </h2>
      <p className="mt-1 text-sm text-text-secondary">
        Kebab-case name, one-line description, and a body with at least two
        sections — When to use, Steps, Pitfalls.
      </p>

      <div className="mt-5 flex max-w-2xl flex-col gap-4">
        {!editing && (
          <label className="flex flex-col gap-1.5">
            <span className="text-sm text-text-secondary">Name</span>
            <input
              type="text"
              value={name}
              placeholder="my-skill-name"
              autoComplete="off"
              onChange={(e) => setName(e.target.value)}
              className={
                "w-full rounded-lg border border-border-primary " +
                "bg-bg-hover-secondary px-2 py-1.5 font-mono text-sm " +
                "text-text-primary placeholder:text-text-secondary " +
                "focus:outline-none focus:ring-1 focus:ring-text-secondary"
              }
            />
            {name.trim() !== "" && nameBad && (
              <span className="text-xs text-red-400">
                Must be kebab-case (lowercase, digits, dashes)
              </span>
            )}
          </label>
        )}

        <label className="flex flex-col gap-1.5">
          <span className="text-sm text-text-secondary">Description</span>
          <input
            type="text"
            value={desc}
            placeholder="One line the agent sees in the skills index"
            autoComplete="off"
            onChange={(e) => setDesc(e.target.value)}
            className={
              "w-full rounded-lg border border-border-primary " +
              "bg-bg-hover-secondary px-2 py-1.5 text-sm text-text-primary " +
              "placeholder:text-text-secondary focus:outline-none " +
              "focus:ring-1 focus:ring-text-secondary"
            }
          />
        </label>

        <label className="flex flex-col gap-1.5">
          <span className="text-sm text-text-secondary">Body</span>
          <textarea
            value={body}
            rows={16}
            placeholder={"## When to use\n\n## Steps\n\n## Pitfalls"}
            onChange={(e) => setBody(e.target.value)}
            className={
              "w-full resize-y rounded-lg border border-border-primary " +
              "bg-bg-hover-secondary px-3 py-2 font-mono text-xs " +
              "leading-5 text-text-primary placeholder:text-text-secondary " +
              "focus:outline-none focus:ring-1 focus:ring-text-secondary"
            }
          />
        </label>

        {err && <p className="text-xs text-red-400">{err}</p>}

        <div className="flex justify-end">
          <button
            type="button"
            onClick={save}
            disabled={busy || nameBad || desc.trim() === "" || body.trim() === ""}
            className={
              "rounded-md bg-accent px-3 py-1.5 text-sm font-medium " +
              "text-bg-primary transition-opacity hover:opacity-90 " +
              "disabled:cursor-not-allowed disabled:opacity-50"
            }
          >
            {busy ? "Saving…" : editing ? "Save changes" : "Create skill"}
          </button>
        </div>
      </div>
    </div>
  );
}
