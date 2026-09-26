import { useEffect, useState } from "react";
import { CgProfile } from "react-icons/cg";
import { FiCheck, FiPlus } from "react-icons/fi";
import { LuSlidersHorizontal } from "react-icons/lu";

import { profileStore, profileLabel, useProfiles } from "../stores/profiles";
import { toast } from "../stores/toast";
import Dropdown from "./Dropdown";

const DEFAULT_ID = "default";
const NAME_MAX = 40;

const BTN =
  "flex w-full items-center gap-2 rounded-xl px-2 py-1.5 text-left text-sm " +
  "transition-colors hover:bg-bg-hover-secondary focus:outline-none " +
  "focus-visible:bg-bg-hover-secondary";

const INPUT =
  "w-full rounded-lg border border-border-primary bg-bg-primary px-2 py-1.5 " +
  "text-sm text-text-primary outline-none placeholder:text-text-tertiary " +
  "focus:border-accent";

type Props = {
  /// The chat the choice applies to, or null when none is open — in which
  /// case the choice is the one the next chat starts on.
  sessionId: string | null;
  onManage: () => void;
};

export default function ProfilePicker({ sessionId, onManage }: Props) {
  const { profiles, activeId } = useProfiles();
  const [creating, setCreating] = useState(false);
  const [name, setName] = useState("");

  useEffect(() => {
    void profileStore.load();
  }, []);

  const current = profiles.find((p) => p.id === activeId);

  const pick = async (id: string) => {
    if (sessionId === null) {
      profileStore.setActive(id);
      return;
    }

    try {
      await profileStore.setForSession(sessionId, id);
    } catch {
      toast.error("Could not change the profile on this chat");
    }
  };

  const create = async () => {
    const n = name.trim();

    if (n === "") {
      return;
    }

    try {
      await profileStore.create(n);
      setName("");
      setCreating(false);
      toast.success(`Created ${n}`);

      if (sessionId !== null) {
        const id = profileStore.getState().activeId;
        await profileStore.setForSession(sessionId, id);
      }
    } catch (err) {
      toast.error(err instanceof Error ? err.message : String(err));
    }
  };

  const panel = creating ? (
    <div className="flex flex-col gap-2 p-1">
      <label
        htmlFor="new-profile-name"
        className="px-1 text-xs text-text-secondary"
      >
        Name
      </label>
      <input
        id="new-profile-name"
        autoFocus
        value={name}
        maxLength={NAME_MAX}
        placeholder="Senior Developer"
        onChange={(e) => setName(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter") {
            e.preventDefault();
            void create();
          }

          if (e.key === "Escape") {
            e.preventDefault();
            setCreating(false);
          }
        }}
        className={INPUT}
      />
      <p className="px-1 text-xs text-text-tertiary">
        You can give it instructions after.
      </p>
      <div className="flex justify-end gap-1.5">
        <button
          type="button"
          onClick={() => {
            setName("");
            setCreating(false);
          }}
          className={
            "rounded-lg px-2.5 py-1 text-xs text-text-secondary " +
            "transition-colors hover:bg-bg-hover-secondary " +
            "hover:text-text-primary"
          }
        >
          Cancel
        </button>
        <button
          type="button"
          disabled={name.trim() === ""}
          onClick={() => void create()}
          className={
            "rounded-lg bg-accent px-2.5 py-1 text-xs font-medium text-white " +
            "transition-opacity hover:opacity-90 " +
            "disabled:cursor-not-allowed disabled:opacity-40"
          }
        >
          Create
        </button>
      </div>
    </div>
  ) : (
    <div className="flex flex-col p-1">
      {profiles.map((p) => {
        const on = p.id === activeId;

        return (
          <button
            key={p.id}
            type="button"
            onClick={() => void pick(p.id)}
            className={BTN}
          >
            <span className="min-w-0 flex-1 truncate">
              {profileLabel(p)}
            </span>
            {on && <FiCheck size={14} className="shrink-0 text-blue-400" />}
          </button>
        );
      })}
      <div className="my-1 h-px bg-border-primary" />
      <button
        type="button"
        onClick={() => setCreating(true)}
        disabled={profiles.length >= 10}
        className={
          BTN +
          " text-text-secondary disabled:cursor-not-allowed disabled:opacity-50"
        }
      >
        <FiPlus size={16} className="shrink-0" />
        <span>Create profile</span>
      </button>
      <button type="button" onClick={onManage} className={BTN}>
        <LuSlidersHorizontal size={16} className="shrink-0" />
        <span>Manage profiles</span>
      </button>
    </div>
  );

  return (
    <Dropdown
      align="right"
      side="bottom"
      items={[]}
      panel={panel}
      panelClassName="w-64"
      trigger={({ open, toggle }) => (
        <button
          type="button"
          onClick={toggle}
          aria-label="Profile"
          aria-expanded={open}
          className={
            "relative flex h-7 w-7 items-center justify-center rounded-md " +
            "transition-colors focus:outline-none focus-visible:bg-bg-hover-primary " +
            `${
              open
                ? "bg-bg-hover-primary text-text-primary"
                : "text-text-secondary hover:bg-bg-hover-primary " +
                  "hover:text-text-primary"
            }`
          }
        >
          <CgProfile size={18} className="text-text-secondary" />
          {current !== undefined && current.id !== DEFAULT_ID && (
            <span
              className={
                "absolute -bottom-0.5 -right-0.5 flex h-3 w-3 items-center " +
                "justify-center rounded-full bg-accent text-[8px] " +
                "font-semibold leading-none text-white"
              }
            >
              {profileLabel(current).charAt(0).toUpperCase()}
            </span>
          )}
        </button>
      )}
    />
  );
}
