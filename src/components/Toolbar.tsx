import { BsLayoutSidebarInset } from "react-icons/bs";
import { CgProfile } from "react-icons/cg";
import { FiChevronDown } from "react-icons/fi";
import { LuArchive, LuDownload, LuTrash2 } from "react-icons/lu";
import { VscSettingsGear } from "react-icons/vsc";
import Dropdown, { type DropdownItem } from "./Dropdown";

type ToolbarProps = {
  onToggleSidebar: () => void;
  sidebarOpen: boolean;
  chatTitle?: string | null;
  onSettings: () => void;
};

export default function Toolbar({
  onToggleSidebar,
  sidebarOpen,
  chatTitle,
  onSettings,
}: ToolbarProps) {
  const menuItems: DropdownItem[] = [
    { label: "Archive", Icon: LuArchive, onClick: () => console.log("archive") },
    { label: "Export", Icon: LuDownload, onClick: () => console.log("export") },
    { label: "Delete", Icon: LuTrash2, onClick: () => console.log("delete") },
  ];

  return (
    <div
      data-tauri-drag-region
      className="flex h-9 w-full shrink-0 items-center justify-between gap-1 bg-transparent"
    >
      <div className="flex h-9 items-center gap-1 pl-2">
        {!sidebarOpen && (
          <button
            type="button"
            onClick={onToggleSidebar}
            className="flex h-7 w-7 items-center justify-center rounded-md text-text-secondary transition-colors hover:bg-bg-hover-primary hover:text-text-primary focus:bg-bg-hover-primary focus:text-text-primary focus-visible:bg-bg-hover-primary focus-visible:text-text-primary"
            aria-label="Expand sidebar"
          >
            <BsLayoutSidebarInset size={18} className="text-text-secondary" />
          </button>
        )}
        {chatTitle && (
          <div className="group flex items-center gap-0.5 rounded-md">
            <button
              type="button"
              className="max-w-[220px] truncate rounded-md px-2 py-1 font-sans text-sm font-medium text-text-primary transition-colors hover:bg-bg-hover-primary focus:outline-none focus-visible:bg-bg-hover-primary"
            >
              {chatTitle}
            </button>
            <Dropdown
              items={menuItems}
              align="left"
              side="bottom"
              trigger={({ open, toggle }) => (
                <button
                  type="button"
                  aria-label="Session options"
                  aria-expanded={open}
                  onClick={toggle}
                  className={`flex h-7 w-7 items-center justify-center rounded-md transition-colors focus:outline-none ${
                    open
                      ? "bg-bg-hover-primary text-text-primary"
                      : "text-text-secondary hover:bg-bg-hover-primary hover:text-text-primary focus-visible:bg-bg-hover-primary"
                  }`}
                >
                  <FiChevronDown size={18} />
                </button>
              )}
            />
          </div>
        )}

      </div>
      <div className="flex h-9 items-center gap-1 pr-2">
        <button
          type="button"
          className="flex h-7 w-7 items-center justify-center rounded-md text-text-secondary transition-colors hover:bg-bg-hover-primary hover:text-text-primary focus:bg-bg-hover-primary focus:text-text-primary focus-visible:bg-bg-hover-primary focus-visible:text-text-primary"
          aria-label="Profile"
        >
          <CgProfile size={18} className="text-text-secondary" />
        </button>
        <button
          type="button"
          onClick={onSettings}
          className="flex h-7 w-7 items-center justify-center rounded-md text-text-secondary transition-colors hover:bg-bg-hover-primary hover:text-text-primary focus:bg-bg-hover-primary focus:text-text-primary focus-visible:bg-bg-hover-primary focus-visible:text-text-primary"
          aria-label="Settings"
        >
          <VscSettingsGear size={18} className="text-text-secondary" />
        </button>
      </div>
    </div>
  );
}
