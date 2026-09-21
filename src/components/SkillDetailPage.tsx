import { useEffect, useRef, useState } from "react";
import { LuArrowLeft, LuChevronDown, LuWand } from "react-icons/lu";

import {
  skillDelete,
  skillGet,
  type Skill,
} from "../lib/ipc";
import { formatRelativeTime, parseDbTime } from "../lib/relativeTime";
import Dropdown from "./Dropdown";

type Props = {
  skill: Skill;
  onBack: () => void;
  onDeleted: (name: string) => void;
};

export default function SkillDetailPage({
  skill,
  onBack,
  onDeleted,
}: Props) {
  const [body, setBody] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  const alive = useRef(true);

  useEffect(() => {
    alive.current = true;
    setBody(null);

    skillGet(skill.name)
      .then((full) => {
        if (alive.current) setBody(full.body);
      })
      .catch(() => {
        if (alive.current) setBody(skill.description);
      });

    return () => {
      alive.current = false;
    };
  }, [skill]);

  const remove = async () => {
    setBusy(true);
    setErr(null);

    try {
      await skillDelete(skill.name);
      onDeleted(skill.name);
    } catch (e) {
      setErr(String(e));
      setBusy(false);
    }
  };

  const lastUsed = parseDbTime(skill.last_used_at);
  const created = parseDbTime(skill.created_at);

  return (
    <div className="py-4">
      <button
        type="button"
        onClick={onBack}
        className="flex items-center gap-1.5 text-sm text-text-secondary hover:text-text-primary cursor-pointer"
      >
        <LuArrowLeft size={15} />
        Skills
      </button>

      <div className="mt-6 flex items-start justify-between gap-4">
        <div className="flex min-w-0 items-center gap-4">
          <div
            className={
              "flex h-14 w-14 shrink-0 items-center justify-center " +
              "rounded-xl border border-border-primary bg-bg-primary " +
              "text-text-primary"
            }
          >
            <LuWand size={24} />
          </div>
          <div className="min-w-0">
            <h2 className="truncate text-xl font-medium text-text-primary">
              {skill.name}
            </h2>
            <p className="text-sm text-text-secondary">{skill.description}</p>
          </div>
        </div>
        <Dropdown
          align="right"
          side="bottom"
          trigger={({ open, toggle }) => (
            <button
              type="button"
              onClick={toggle}
              aria-label="Skill actions"
              className={
                "flex h-8 items-center rounded-md border " +
                "border-border-primary bg-bg-hover-secondary px-2 " +
                "text-text-secondary hover:text-text-primary " +
                "focus:outline-none cursor-pointer"
              }
            >
              <LuChevronDown
                size={13}
                className={`transition-transform ${open ? "rotate-180" : ""}`}
              />
            </button>
          )}
          items={[
            {
              label: "Delete skill",
              danger: true,
              disabled: busy,
              onClick: () => {
                void remove();
              },
            },
          ]}
        />
      </div>

      <div className="mt-8 flex gap-8">
        <div className="min-w-0 flex-1">
          {body !== null ? (
            <pre className="whitespace-pre-wrap font-mono text-xs leading-5 text-text-primary">
              {body}
            </pre>
          ) : (
            <p className="text-sm text-text-secondary">Loading…</p>
          )}

          {err && <p className="mt-3 text-xs text-red-400">{err}</p>}
        </div>

        <div className="w-44 shrink-0">
          <p className="text-xs font-medium uppercase tracking-wide text-text-secondary">
            Source
          </p>
          <p className="mt-0.5 text-xs text-text-primary">{skill.source}</p>

          <p className="mt-4 text-xs font-medium uppercase tracking-wide text-text-secondary">
            Origin
          </p>
          <p className="mt-0.5 truncate text-xs text-text-primary">
            {skill.origin ?? "—"}
          </p>

          <p className="mt-4 text-xs font-medium uppercase tracking-wide text-text-secondary">
            Used
          </p>
          <p className="mt-0.5 text-xs text-text-primary">
            {skill.use_count} time{skill.use_count === 1 ? "" : "s"}
          </p>

          {lastUsed !== null && (
            <>
              <p className="mt-4 text-xs font-medium uppercase tracking-wide text-text-secondary">
                Last used
              </p>
              <p className="mt-0.5 text-xs text-text-primary">
                {formatRelativeTime(lastUsed)}
              </p>
            </>
          )}

          {created !== null && (
            <>
              <p className="mt-4 text-xs font-medium uppercase tracking-wide text-text-secondary">
                Created
              </p>
              <p className="mt-0.5 text-xs text-text-primary">
                {formatRelativeTime(created)}
              </p>
            </>
          )}
        </div>
      </div>
    </div>
  );
}
