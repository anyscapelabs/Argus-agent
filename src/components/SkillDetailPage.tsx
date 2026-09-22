import { useEffect, useRef, useState } from "react";
import { FiCode, FiEye, FiFileText, FiFolder, FiMoreVertical } from "react-icons/fi";
import { LuArrowLeft, LuPencil, LuWand } from "react-icons/lu";

import {
  skillDelete,
  skillReadFile,
  skillFiles,
  type Skill,
} from "../lib/ipc";
import { formatRelativeTime, parseDbTime } from "../lib/relativeTime";
import { renderSkillMd } from "../lib/skillMd";
import Dropdown from "./Dropdown";

type Props = {
  skill: Skill;
  onBack: () => void;
  onEdit: (skill: Skill) => void;
  onDeleted: (name: string) => void;
};

function isMd(path: string): boolean {
  return path.endsWith(".md");
}

export default function SkillDetailPage({
  skill,
  onBack,
  onEdit,
  onDeleted,
}: Props) {
  const [files, setFiles] = useState<string[]>(["SKILL.md"]);
  const [file, setFile] = useState("SKILL.md");
  const [fileText, setFileText] = useState<string | null>(null);
  const [tab, setTab] = useState<"overview" | "contents">("overview");
  const [raw, setRaw] = useState(false);
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  const alive = useRef(true);

  useEffect(() => {
    alive.current = true;
    setTab("overview");
    setFile("SKILL.md");
    setFiles(["SKILL.md"]);
    setFileText(null);

    skillFiles(skill.name)
      .then((list) => {
        if (alive.current) setFiles(list);
      })
      .catch(() => {});

    return () => {
      alive.current = false;
    };
  }, [skill]);

  useEffect(() => {
    if (tab !== "contents") return;

    let local = true;
    setFileText(null);

    skillReadFile(skill.name, file)
      .then((text) => {
        if (local) setFileText(text);
      })
      .catch((e) => {
        if (local) setFileText(String(e));
      });

    return () => {
      local = false;
    };
  }, [tab, skill, file]);

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
  const subLine =
    `by ${skill.origin ?? skill.source} · used ${skill.use_count} time${skill.use_count === 1 ? "" : "s"}` +
    (lastUsed !== null ? ` · updated ${formatRelativeTime(lastUsed)}` : "");

  const tree = files.reduce<{ label: string; files: string[] }[]>((acc, f) => {
    const slash = f.indexOf("/");

    if (slash === -1) {
      acc.push({ label: "", files: [f] });
      return acc;
    }

    const dir = f.slice(0, slash);
    const last = acc[acc.length - 1];

    if (last && last.label === dir) {
      last.files.push(f);
    } else {
      acc.push({ label: dir, files: [f] });
    }

    return acc;
  }, []);

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
            <p className="truncate text-sm text-text-secondary">{subLine}</p>
          </div>
        </div>
        <Dropdown
          align="right"
          side="bottom"
          trigger={({ toggle }) => (
            <button
              type="button"
              onClick={toggle}
              aria-label="Skill actions"
              className={
                "flex h-8 w-8 items-center justify-center rounded-md " +
                "border border-border-primary bg-bg-hover-secondary " +
                "text-text-secondary hover:text-text-primary " +
                "focus:outline-none cursor-pointer"
              }
            >
              <FiMoreVertical size={15} />
            </button>
          )}
          items={[
            {
              label: "Edit skill",
              Icon: LuPencil,
              onClick: () => onEdit(skill),
            },
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

      <div className="mt-5 flex gap-5 border-b border-border-primary">
        {(["overview", "contents"] as const).map((t) => (
          <button
            key={t}
            type="button"
            onClick={() => setTab(t)}
            className={
              "-mb-px border-b-2 px-1 pb-2 text-sm transition-colors cursor-pointer " +
              `${
                tab === t
                  ? "border-text-primary font-medium text-text-primary"
                  : "border-transparent text-text-secondary hover:text-text-primary"
              }`
            }
          >
            {t === "overview" ? "Overview" : `Contents · ${files.length}`}
          </button>
        ))}
      </div>

      {tab === "overview" && (
        <div className="mt-5 flex gap-8">
          <div className="min-w-0 flex-1">
            <p className="text-sm leading-6 text-text-primary">
              {skill.description}
            </p>
            <div
              className={
                "mt-5 flex items-start gap-2 rounded-lg border " +
                "border-border-primary bg-bg-primary px-3 py-2.5"
              }
            >
              <p className="text-xs leading-relaxed text-text-secondary">
                Argus reads this skill when its entry looks relevant to the
                task, following the steps and pitfalls inside to work the same
                way every time.
              </p>
            </div>
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
      )}

      {tab === "contents" && (
        <div className="mt-5 flex h-[62vh] gap-0 overflow-hidden rounded-lg border border-border-primary">
          <div className="w-52 shrink-0 overflow-y-auto border-r border-border-primary px-1.5 py-2">
            {tree.map((grp, gi) => (
              <div key={gi} className="mb-1">
                {grp.label !== "" && (
                  <div className="flex items-center gap-1.5 px-2 py-1 text-xs text-text-secondary">
                    <FiFolder size={12} />
                    {grp.label}
                  </div>
                )}
                {grp.files.map((f) => {
                  const name = grp.label === "" ? f : f.slice(grp.label.length + 1);

                  return (
                    <button
                      key={f}
                      type="button"
                      onClick={() => setFile(f)}
                      className={
                        "flex w-full items-center gap-1.5 rounded-md px-2 py-1 " +
                        "text-left text-xs transition-colors cursor-pointer " +
                        `${
                          file === f
                            ? "bg-bg-hover-secondary text-text-primary"
                            : "text-text-secondary hover:text-text-primary"
                        } ${grp.label !== "" ? "pl-6" : ""}`
                      }
                    >
                      <FiFileText size={11} className="shrink-0" />
                      <span className="truncate">{name}</span>
                    </button>
                  );
                })}
              </div>
            ))}
          </div>

          <div className="flex min-w-0 flex-1 flex-col">
            <div className="flex items-center justify-between border-b border-border-primary px-3 py-1.5">
              <span className="truncate font-mono text-[11px] text-text-secondary">
                /skills/{skill.name}/{file}
              </span>
              <div className="flex items-center gap-1">
                <button
                  type="button"
                  onClick={() => setRaw(false)}
                  aria-label="Rendered view"
                  className={
                    "flex h-6 w-6 items-center justify-center rounded-md " +
                    "transition-colors cursor-pointer " +
                    `${
                      raw
                        ? "text-text-secondary hover:text-text-primary"
                        : "bg-bg-hover-secondary text-text-primary"
                    }`
                  }
                >
                  <FiEye size={13} />
                </button>
                <button
                  type="button"
                  onClick={() => setRaw(true)}
                  aria-label="Raw view"
                  className={
                    "flex h-6 w-6 items-center justify-center rounded-md " +
                    "transition-colors cursor-pointer " +
                    `${
                      raw
                        ? "bg-bg-hover-secondary text-text-primary"
                        : "text-text-secondary hover:text-text-primary"
                    }`
                  }
                >
                  <FiCode size={13} />
                </button>
              </div>
            </div>
            <div className="min-h-0 flex-1 overflow-y-auto px-5 py-3">
              {fileText === null ? (
                <p className="text-sm text-text-secondary">Loading…</p>
              ) : isMd(file) && !raw ? (
                renderSkillMd(fileText)
              ) : (
                <pre className="whitespace-pre-wrap font-mono text-xs leading-5 text-text-primary">
                  {fileText}
                </pre>
              )}
            </div>
          </div>
        </div>
      )}

      {err && <p className="mt-3 text-xs text-red-400">{err}</p>}
    </div>
  );
}
