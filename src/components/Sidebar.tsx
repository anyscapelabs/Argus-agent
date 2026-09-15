import { BsLayoutSidebarInset } from "react-icons/bs";
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
  activeSessionId,
  sessions,
  onArchive,
  onExport,
  onDelete,
}: SidebarProps) {
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
        </div>
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
              <button
                key={label}
                type="button"
                onClick={() => onNavigate(view)}
                className={
                  "flex items-center gap-2 rounded-md px-2 py-1 " +
                  "transition-colors focus:outline-none " +
                  `${
                    isActive
                      ? "bg-bg-hover-primary text-text-primary"
                      : "text-text-secondary hover:bg-bg-hover-primary " +
                        "focus-visible:bg-bg-hover-primary"
                  }`
                }
              >
                <Icon
                  size={16}
                  className={
                    isActive ? "text-text-primary" : "text-text-secondary"
                  }
                />
                <span className="text-sm font-medium">{label}</span>
              </button>
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
    </aside>
  );
}
