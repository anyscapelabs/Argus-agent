import { useState } from "react";
import { IoSparklesOutline } from "react-icons/io5";

export default function SettingsSidebar() {
  const [active, setActive] = useState(true);

  return (
    <aside className="flex w-56 shrink-0 flex-col border-r border-border-primary bg-bg-secondary" aria-label="Settings sidebar">
      <nav className="flex flex-col gap-0.5 p-2">
        <button
          type="button"
          onClick={() => setActive(true)}
          aria-current={active ? "page" : undefined}
          className={`flex items-center gap-2 rounded-md px-2 py-1 text-sm font-medium transition-colors ${
            active
              ? "bg-bg-hover-secondary text-text-primary"
              : "text-text-secondary hover:bg-bg-hover-secondary hover:text-text-primary"
          }`}
        >
          <IoSparklesOutline size={16} className={active ? "text-text-primary" : "text-text-secondary"} />
          <span>Models</span>
        </button>
      </nav>
    </aside>
  );
}
