import { useEffect, useState } from "react";
import { LuBrain, LuPlus, LuSearch } from "react-icons/lu";

import type { Skill } from "../lib/ipc";
import { skillDelete, skillList, skillSearch } from "../lib/ipc";
import { toast } from "../stores/toast";
import SkillCard from "./SkillCard";
import SkillDetailPage from "./SkillDetailPage";
import SkillEditorPage from "./SkillEditorPage";

export default function SkillsPage() {
  const [skills, setSkills] = useState<Skill[]>([]);
  const [query, setQuery] = useState("");
  const [err, setErr] = useState<string | null>(null);
  const [sel, setSel] = useState<Skill | null>(null);
  const [editing, setEditing] = useState<Skill | null>(null);
  const [creating, setCreating] = useState(false);

  useEffect(() => {
    let alive = true;

    skillList()
      .then((rows) => {
        if (alive) setSkills(rows);
      })
      .catch((err) => {
        if (alive) setErr(String(err));
      });

    return () => {
      alive = false;
    };
  }, []);

  useEffect(() => {
    if (query.trim() === "") {
      skillList()
        .then(setSkills)
        .catch(() => {});

      return;
    }

    const t = setTimeout(() => {
      skillSearch(query.trim())
        .then(setSkills)
        .catch(() => {});
    }, 200);

    return () => clearTimeout(t);
  }, [query]);

  const remove = async (name: string) => {
    try {
      await skillDelete(name);
      toast.success("Skill deleted");
    } catch {
      toast.error("Delete failed");
      return;
    }

    setSkills((prev) => prev.filter((s) => s.name !== name));
  };

  if (creating) {
    return (
      <div className="mx-auto w-full max-w-2xl py-4">
        <SkillEditorPage
          skill={null}
          onBack={() => setCreating(false)}
          onSaved={(s) => {
            setCreating(false);
            setSkills((prev) => [s, ...prev.filter((x) => x.name !== s.name)]);
          }}
        />
      </div>
    );
  }

  if (editing) {
    return (
      <div className="mx-auto w-full max-w-2xl py-4">
        <SkillEditorPage
          skill={editing}
          onBack={() => setEditing(null)}
          onSaved={(s) => {
            setEditing(null);
            setSkills((prev) => prev.map((x) => (x.name === s.name ? s : x)));
          }}
        />
      </div>
    );
  }

  if (sel) {
    return (
      <div className="mx-auto w-full max-w-2xl py-4">
        <SkillDetailPage
          skill={sel}
          onBack={() => setSel(null)}
          onEdit={setEditing}
          onDeleted={(name) => {
            void remove(name);
            setSel(null);
          }}
        />
      </div>
    );
  }

  return (
    <div className="mx-auto w-full max-w-2xl py-4">
      <div className="mb-1 flex items-center justify-between">
        <h1 className="text-2xl font-medium text-text-primary">Skills</h1>
        <button
          type="button"
          onClick={() => setCreating(true)}
          className={
            "inline-flex items-center justify-center gap-1 rounded-md " +
            "bg-accent px-2 py-1 text-xs font-medium text-bg-primary " +
            "transition-opacity hover:opacity-90 cursor-pointer"
          }
          aria-label="Add skill"
        >
          <LuPlus size={14} />
          Add
        </button>
      </div>
      <p className="mb-5 text-sm font-medium text-text-secondary">
        Equip Argus with specialized capabilities.
      </p>
      <div
        className={
          "flex h-10 w-full items-center rounded-lg border " +
          "border-border-primary bg-bg-secondary px-4"
        }
      >
        <LuSearch size={18} className="shrink-0 text-text-secondary" />
        <input
          type="text"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder="Search skills..."
          className={
            "ml-2 flex-1 bg-transparent text-sm text-text-primary " +
            "outline-none placeholder:text-text-secondary"
          }
        />
      </div>
      {err !== null && (
        <p className="mt-4 text-sm text-red-400">{err}</p>
      )}
      {err === null && skills.length === 0 && (
        <div className="flex flex-col items-center gap-3 px-6 py-12 text-center">
          <LuBrain size={28} className="text-text-secondary" />
          <div className="flex flex-col gap-1">
            <p className="text-sm font-medium text-text-primary">
              {query.trim() === "" ? "No skills yet" : "No skills match"}
            </p>
            <p className="max-w-xs text-xs leading-relaxed text-text-secondary">
              {query.trim() === ""
                ? "Ask Argus to remember something reusable and it will show up here."
                : `Nothing found for "${query.trim()}". Try a shorter or different name.`}
            </p>
          </div>
        </div>
      )}
      <div className="mt-6 flex flex-col gap-1">
        {skills.map((skill) => (
          <SkillCard key={skill.name} skill={skill} onOpen={setSel} />
        ))}
      </div>
    </div>
  );
}
