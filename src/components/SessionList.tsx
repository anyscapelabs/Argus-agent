import { useState } from "react";
import { FiMoreVertical } from "react-icons/fi";
import { LuArchive, LuDownload, LuTrash2 } from "react-icons/lu";
import Dropdown, { type DropdownItem } from "./Dropdown";

export type SessionStatus = "live" | "inactive";

export type Session = {
  id: string;
  title: string;
  status: SessionStatus;
};

function StatusDot({ status }: { status: SessionStatus }) {
  if (status === "live") {
    return (
      <span
        aria-label="Live session"
        className="relative inline-flex h-2 w-2 shrink-0"
      >
        <span className="absolute inline-flex h-full w-full animate-ping rounded-full bg-emerald-400 opacity-60" />
        <span className="relative inline-flex h-2 w-2 rounded-full bg-emerald-400" />
      </span>
    );
  }
  return (
    <span
      aria-label="Inactive session"
      className="inline-flex h-2 w-2 shrink-0 rounded-full bg-text-secondary/40"
    />
  );
}

type SessionListProps = {
  onSelect: (sessionId: string) => void;
  activeSessionId: string | null;
  sessions: Session[];
};

const menuItems = (sessionId: string): DropdownItem[] => [
  { label: "Archive", Icon: LuArchive, onClick: () => console.log("archive", sessionId) },
  { label: "Export", Icon: LuDownload, onClick: () => console.log("export", sessionId) },
  { label: "Delete", Icon: LuTrash2, onClick: () => console.log("delete", sessionId) },
];

export default function SessionList({
  onSelect,
  activeSessionId,
  sessions,
}: SessionListProps) {
  const [open, setOpen] = useState(true);

  return (
    <div className="mt-4 flex flex-col">
      <button
        type="button"
        onClick={() => setOpen((value) => !value)}
        className="mx-2 flex items-center rounded-md px-2 py-1 text-text-secondary transition-colors hover:bg-bg-hover-primary focus:bg-bg-hover-primary focus-visible:bg-bg-hover-primary"
        aria-expanded={open}
        aria-controls="session-list-panel"
      >
        <span className="text-xs font-medium">Sessions</span>
      </button>
      {open && (
        <div
          id="session-list-panel"
          className="flex flex-col gap-0.5 px-2"
        >
          {sessions.map(({ id, title, status }) => {
            const isActive = id === activeSessionId;
            return (
              <div
                key={id}
                className={`group flex items-center gap-2 rounded-md px-2 py-1 transition-colors ${
                  isActive
                    ? "bg-bg-hover-primary text-text-primary"
                    : "text-text-secondary hover:bg-bg-hover-primary"
                }`}
              >
                <StatusDot status={status} />
                <button
                  type="button"
                  onClick={() => onSelect(id)}
                  className="flex-1 truncate text-left text-sm font-medium focus:outline-none"
                >
                  {title}
                </button>
                <Dropdown
                  items={menuItems(id)}
                  align="right"
                  side="bottom"
                  trigger={({ open: isOpen, toggle }) => (
                    <button
                      type="button"
                      onClick={(event) => {
                        event.stopPropagation();
                        toggle();
                      }}
                      aria-label={`Session menu for ${title}`}
                      aria-haspopup="menu"
                      aria-expanded={isOpen}
                      className={`flex h-6 w-6 items-center justify-center rounded-md transition-opacity focus:outline-none ${
                        isOpen
                          ? "bg-bg-secondary text-text-primary opacity-100"
                          : "text-text-secondary opacity-0 hover:text-text-primary focus:opacity-100 group-hover:opacity-100 group-focus-within:opacity-100"
                      }`}
                    >
                      <FiMoreVertical size={14} />
                    </button>
                  )}
                />
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}
