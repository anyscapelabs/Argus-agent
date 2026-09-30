import { useMemo, useState } from "react";
import { BsLayoutSidebarInset } from "react-icons/bs";
import { FiArrowLeft } from "react-icons/fi";
import { IoSearchOutline } from "react-icons/io5";
import { LuUnplug } from "react-icons/lu";
import { PiToolbox } from "react-icons/pi";
import {
  RiAiAgentLine,
  RiBrainLine,
  RiToolsLine,
} from "react-icons/ri";
import { VscFolderLibrary } from "react-icons/vsc";

import type { View } from "../App";
import {
  SETTINGS_TABS,
  type SettingsTab,
  type TabIcon,
} from "./settings/tabs";
import ScrollBox from "./ScrollBox";
import SessionList, { type Session } from "./SessionList";

const TABS: { label: string; view: View; Icon: typeof RiBrainLine }[] = [
  { label: "Memory", view: "memory", Icon: RiBrainLine },
  { label: "Skills", view: "skills", Icon: RiToolsLine },
  { label: "Library", view: "library", Icon: VscFolderLibrary },
  { label: "Projects", view: "projects", Icon: PiToolbox },
  { label: "Connectors", view: "connectors", Icon: LuUnplug },
];

type SidebarProps = {
  onToggle: () => void;
  open: boolean;
  onNewAgent: () => void;
  onSelectSession: (sessionId: string) => void;
  onNavigate: (view: View) => void;
  activeView: View;
  settingsTab: SettingsTab;
  onSettingsTab: (tab: SettingsTab) => void;
  onBack: () => void;
  activeSessionId: string | null;
  sessions: Session[];
  onArchive: (sessionId: string) => void;
  onExport: (sessionId: string) => void;
  onDelete: (sessionId: string) => void;
};

export default function Sidebar({
  onToggle,
  open,
  onNewAgent,
  onSelectSession,
  onNavigate,
  activeView,
  settingsTab,
  onSettingsTab,
  onBack,
  activeSessionId,
  sessions,
  onArchive,
  onExport,
  onDelete,
}: SidebarProps) {
  const inSettings = activeView === "settings";
  const [settingsQuery, setSettingsQuery] = useState("");
  const settingsGroups = useMemo(() => {
    const query = settingsQuery.trim().toLowerCase();
    return SETTINGS_TABS.filter(({ label, group }) =>
      `${label} ${group}`.toLowerCase().includes(query),
    ).reduce<Record<string, typeof SETTINGS_TABS>>((groups, item) => {
      groups[item.group] = [...(groups[item.group] ?? []), item];
      return groups;
    }, {});
  }, [settingsQuery]);

  return (
    <aside
      className={
        "flex h-full shrink-0 flex-col overflow-hidden " +
        "bg-bg-primary transition-[width] duration-300 ease-in-out " +
        `${open ? "border-r border-border-primary" : ""}`
      }
      style={{ width: open ? 256 : 0 }}
      aria-hidden={!open}
    >
      <div
        className="flex h-full w-64 flex-col"
        style={{
          opacity: open ? 1 : 0,
          transition: "opacity 200ms ease-in-out",
          pointerEvents: open ? "auto" : "none",
        }}
      >
        <div className="flex h-9 items-center gap-1 px-2">
          <button
            type="button"
            onClick={onToggle}
            className={
              "flex h-7 w-7 items-center justify-center rounded-md " +
              "text-text-secondary transition-colors " +
              "hover:bg-bg-hover-primary hover:text-text-primary " +
              "focus:bg-bg-hover-primary focus:text-text-primary " +
              "focus-visible:bg-bg-hover-primary " +
              "focus-visible:text-text-primary"
            }
            aria-label="Toggle sidebar"
          >
            <BsLayoutSidebarInset
              size={18}
              className="text-text-secondary"
            />
          </button>
          {inSettings ? (
            <button
              type="button"
              onClick={onBack}
              className={
                "flex h-7 items-center gap-1 rounded-md pl-1 pr-2 " +
                "text-xs font-medium text-text-secondary " +
                "transition-colors hover:bg-bg-hover-primary " +
                "hover:text-text-primary"
              }
              aria-label="Back out of settings"
            >
              <FiArrowLeft size={14} />
              <span>Back to app</span>
            </button>
          ) : (
            <button
              type="button"
              className={
                "flex h-7 w-7 items-center justify-center rounded-md " +
                "text-text-secondary transition-colors " +
                "hover:bg-bg-hover-primary hover:text-text-primary " +
                "focus:bg-bg-hover-primary focus:text-text-primary " +
                "focus-visible:bg-bg-hover-primary " +
                "focus-visible:text-text-primary"
              }
              aria-label="Search"
            >
              <IoSearchOutline size={18} className="text-text-secondary" />
            </button>
          )}
        </div>
        {inSettings && (
          <div className="px-3 pb-3 pt-5">
            <h2 className="px-1 text-[15px] font-normal text-text-primary">
              Settings
            </h2>
            <label className="relative mt-4 block">
              <IoSearchOutline
                size={15}
                className="pointer-events-none absolute left-3 top-1/2 -translate-y-1/2 text-text-tertiary"
              />
              <input
                value={settingsQuery}
                onChange={(event) => setSettingsQuery(event.target.value)}
                placeholder="Search settings"
                aria-label="Search settings"
                className="h-9 w-full rounded-lg border border-border-primary bg-bg-secondary pl-9 pr-3 text-sm text-text-primary placeholder:text-text-tertiary outline-none transition focus:border-text-tertiary"
              />
            </label>
          </div>
        )}

        <ScrollBox className="min-h-0 flex-1">
            {inSettings ? (
              <nav className="flex flex-col gap-4 px-2 pb-4">
                {Object.entries(settingsGroups).map(([group, items]) => (
                  <section key={group}>
                    <h3 className="px-2 pb-1.5 text-xs text-text-tertiary">
                      {group}
                    </h3>
                    <div className="flex flex-col gap-0.5">
                      {items.map(({ tab, label, Icon }) => (
                        <SidebarRow
                          key={tab}
                          label={label}
                          Icon={Icon}
                          active={tab === settingsTab}
                          onClick={() => onSettingsTab(tab)}
                        />
                      ))}
                    </div>
                  </section>
                ))}
                {Object.keys(settingsGroups).length === 0 && (
                  <p className="px-3 py-4 text-sm text-text-tertiary">
                    No settings found
                  </p>
                )}
            </nav>
          ) : (
            <div className="flex flex-col pb-4">
              <button
                type="button"
                onClick={onNewAgent}
                className={
                  "mx-2 mt-1 flex items-center gap-2 rounded-md px-2 py-1 " +
                  "transition-colors focus:outline-none " +
                  `${
                    activeView === "new-agent"
                      ? "bg-bg-hover-primary text-text-primary"
                      : "text-text-secondary hover:bg-bg-hover-primary"
                  }`
                }
              >
                <RiAiAgentLine
                  size={16}
                  className={
                    activeView === "new-agent"
                      ? "text-text-primary"
                      : "text-text-secondary"
                  }
                />
                <span className="text-sm font-medium">New Agent</span>
              </button>
              <nav className="flex flex-col gap-0.5 px-2">
                {TABS.map(({ label, view, Icon }) => {
                  const isActive = activeView === view;

                  return (
                    <SidebarRow
                      key={label}
                      label={label}
                      Icon={Icon}
                      active={isActive}
                      onClick={() => onNavigate(view)}
                    />
                  );
                })}
              </nav>
              <SessionList
                onSelect={onSelectSession}
                activeSessionId={activeSessionId}
                sessions={sessions}
                onArchive={onArchive}
                onExport={onExport}
                onDelete={onDelete}
            />
            </div>
          )}
        </ScrollBox>
      </div>
    </aside>
  );
}

type SidebarRowProps = {
  label: string;
  Icon: TabIcon;
  active: boolean;
  onClick: () => void;
};

function SidebarRow({ label, Icon, active, onClick }: SidebarRowProps) {
  return (
    <button
      type="button"
      onClick={onClick}
      aria-current={active ? "page" : undefined}
      className={
        "flex items-center gap-2 rounded-md px-2 py-1 transition-colors " +
        "focus:outline-none " +
        `${
          active
            ? "bg-bg-hover-primary text-text-primary"
            : "text-text-secondary hover:bg-bg-hover-primary " +
              "focus-visible:bg-bg-hover-primary"
        }`
      }
    >
      <Icon
        size={16}
        className={active ? "text-text-primary" : "text-text-secondary"}
      />
      <span className="text-sm font-medium">{label}</span>
    </button>
  );
}
