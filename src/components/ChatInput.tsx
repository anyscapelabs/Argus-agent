import { useEffect, useRef } from "react";
import { HiArrowUp, HiPlus } from "react-icons/hi";
import { LuFolderOpen, LuGlobe, LuLibrary, LuPlug } from "react-icons/lu";
import { RiAttachment2 } from "react-icons/ri";
import Dropdown, { type DropdownItem } from "./Dropdown";

const COLLAPSED_HEIGHT = 40;
const MAX_HEIGHT = 240;

type ChatInputProps = {
  value: string;
  onChange: (next: string) => void;
  onSubmit: () => void;
  placeholder?: string;
};

export default function ChatInput({
  value,
  onChange,
  onSubmit,
  placeholder = "What can i do for you?",
}: ChatInputProps) {
  const textareaRef = useRef<HTMLTextAreaElement | null>(null);

  useEffect(() => {
    const textarea = textareaRef.current;
    if (!textarea) return;

    textarea.style.height = "auto";
    const next = Math.min(textarea.scrollHeight, MAX_HEIGHT);
    textarea.style.height = `${next}px`;
    textarea.style.overflowY = textarea.scrollHeight > MAX_HEIGHT ? "auto" : "hidden";
  }, [value]);

  const empty = value.trim().length === 0;

  const addItems: DropdownItem[] = [
    { label: "Add files or photos", Icon: RiAttachment2, onClick: () => console.log("attach") },
    { label: "Add from library", Icon: LuLibrary, hasSubmenu: true, onClick: () => console.log("library") },
    { label: "Add project", Icon: LuFolderOpen, hasSubmenu: true, onClick: () => console.log("project") },
    { label: "Connector", Icon: LuPlug, hasSubmenu: true, onClick: () => console.log("connector") },
    { label: "Web search", Icon: LuGlobe, toggleable: true, onClick: () => console.log("web") },
  ];

  return (
    <div
      className={`flex w-[700px] items-center gap-2 bg-bg-secondary p-1 transition-[border-radius] ${
        value.includes("\n") ? "items-end rounded-xl" : "rounded-full"
      }`}
    >
      <Dropdown
        items={addItems}
        trigger={({ open, toggle }) => (
          <button
            type="button"
            aria-label="Add"
            aria-expanded={open}
            onClick={toggle}
            className={`flex h-10 w-10 shrink-0 items-center justify-center rounded-full transition-colors focus:outline-none ${
              open
                ? "bg-bg-hover-secondary text-text-primary"
                : "text-text-secondary hover:bg-bg-hover-secondary hover:text-text-primary focus-visible:bg-bg-hover-secondary"
            }`}
          >
            <HiPlus size={18} />
          </button>
        )}
      />
      <textarea
        ref={textareaRef}
        value={value}
        onChange={(event) => onChange(event.target.value)}
        onKeyDown={(event) => {
          if (event.key === "Enter" && !event.shiftKey) {
            event.preventDefault();
            if (!empty) onSubmit();
          }
        }}
        placeholder={placeholder}
        rows={1}
        style={{ height: COLLAPSED_HEIGHT, lineHeight: "20px" }}
        className="flex-1 resize-none bg-transparent px-1 py-2 text-sm font-medium text-text-primary placeholder:text-text-secondary placeholder:font-normal focus:outline-none"
      />
      <button
        type="button"
        aria-label="Send"
        disabled={empty}
        onClick={() => {
          if (empty) return;
          onSubmit();
        }}
        className="flex h-10 w-10 shrink-0 items-center justify-center rounded-full bg-bg-hover-secondary text-text-secondary transition-colors hover:bg-bg-hover-primary focus:outline-none focus-visible:bg-bg-hover-primary disabled:bg-bg-hover-secondary disabled:text-text-secondary enabled:bg-white enabled:text-bg-primary enabled:hover:opacity-90"
      >
        <HiArrowUp size={18} />
      </button>
    </div>
  );
}
