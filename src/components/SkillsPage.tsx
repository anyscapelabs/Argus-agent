import { useEffect, useState } from "react";
import { FiChevronDown } from "react-icons/fi";
import { LuSearch } from "react-icons/lu";

import type { Skill } from "../lib/ipc";
import { skillDelete, skillList, skillSearch } from "../lib/ipc";
import SkillCard from "./SkillCard";

type SkillsPageProps = {
  onAddSkill?: () => void;
};

export default function SkillsPage({ onAddSkill }: SkillsPageProps) {
  const [skills, setSkills] = useState<Skill[]>([]);
  const [query, setQuery] = useState("");
  const [err, setErr] = useState<string | null>(null);

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
    } catch {
      return;
    }

    setSkills((prev) => prev.filter((s) => s.name !== name));
  };

  return (
    <div className="mx-auto w-full max-w-2xl py-4">
      <div className="mb-1 flex items-center justify-between">
        <h1 className="text-2xl font-medium text-text-primary">Skills</h1>
        <button
          type="button"
          onClick={onAddSkill ?? (() => {})}
          className={
            "inline-flex items-center justify-center gap-1 rounded-lg " +
            "bg-accent px-2 py-1 text-xs font-medium text-bg-primary " +
            "transition-opacity hover:opacity-90 cursor-pointer"
          }
          aria-label="Add skill"
        >
          Add
          <FiChevronDown size={16} />
        </button>
      </div>
      <p className="mb-5 text-sm font-medium text-text-secondary">
        Equip Argus with specialized capabilities.
      </p>
      <div
        className={
          "flex h-10 w-full items-center rounded-full border " +
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
      <div className="mt-6 grid grid-cols-2 gap-4">
        {skills.map((skill) => (
          <SkillCard key={skill.name} skill={skill} onDelete={remove} />
        ))}
      </div>
    </div>
  );
}
