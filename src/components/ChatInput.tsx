import { useEffect, useRef, useState } from "react";
import { FiChevronDown } from "react-icons/fi";
import { HiArrowUp, HiPlus } from "react-icons/hi";
import {
  LuFolderOpen,
  LuGlobe,
  LuLibrary,
  LuMic,
  LuPlug,
  LuShieldCheck,
} from "react-icons/lu";
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

const MODELS = [
  { id: "fable-5.1", provider: "Fable", name: "Fable 5.1" },
  { id: "fable-5.6", provider: "Fable", name: "5.6 Terra" },
  { id: "fable-4", provider: "Fable", name: "Fable 4" },
] as const;

type Permission = "Always allow" | "Ask always";

export default function ChatInput({
  value,
  onChange,
  onSubmit,
  placeholder = "Work with Argus",
}: ChatInputProps) {
  const textareaRef = useRef<HTMLTextAreaElement | null>(null);
  const [selectedModel, setSelectedModel] = useState<(typeof MODELS)[number]>(MODELS[0]);
  const [permission, setPermission] = useState<Permission>("Always allow");

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

  const permissionItems: DropdownItem[] = [
    { label: "Always allow", onClick: () => setPermission("Always allow"), active: permission === "Always allow" },
    { label: "Ask always", onClick: () => setPermission("Ask always"), active: permission === "Ask always" },
  ];

  const modelItems: DropdownItem[] = MODELS.map((m) => ({
    label: m.name,
    onClick: () => setSelectedModel(m),
    active: selectedModel.id === m.id,
  }));

  return (
    <div className="flex w-[700px] max-w-full flex-col rounded-2xl bg-bg-secondary border border-border-primary p-3 shadow-4xl">
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
        className="min-h-[44px] w-full resize-none bg-transparent px-2 text-sm font-medium text-text-primary placeholder:text-text-secondary placeholder:font-normal focus:outline-none"
      />

      <div className="mt-2 flex items-center justify-between gap-2">
        <div className="flex items-center gap-2">
          <Dropdown
            items={addItems}
            side="top"
            align="left"
            trigger={({ open, toggle }) => (
              <button
                type="button"
                aria-label="Add"
                aria-expanded={open}
                onClick={toggle}
                className={`flex h-8 w-8 shrink-0 items-center justify-center rounded-full transition-colors focus:outline-none ${
                  open
                    ? "bg-bg-hover-secondary text-text-primary"
                    : "text-text-secondary hover:bg-bg-hover-secondary hover:text-text-primary focus-visible:bg-bg-hover-secondary"
                }`}
              >
                <HiPlus size={16} />
              </button>
            )}
          />

          <Dropdown
            items={permissionItems}
            side="top"
            align="left"
            trigger={({ open, toggle }) => (
              <button
                type="button"
                onClick={toggle}
                aria-expanded={open}
                className="inline-flex h-8 items-center justify-center gap-1.5 rounded-full bg-transparent px-3 text-sm font-medium text-text-secondary hover:bg-bg-hover-secondary hover:text-text-primary transition-colors"
              >
                <LuShieldCheck size={14} className="shrink-0" />
                {permission === "Always allow" ? "Approve for me" : permission}
                <FiChevronDown size={12} className={`shrink-0 transition-transform ${open ? "rotate-180" : ""}`} />
              </button>
            )}
          />
        </div>

        <div className="flex items-center gap-2">
          <Dropdown
            items={modelItems}
            side="top"
            align="right"
            trigger={({ open, toggle }) => (
              <button
                type="button"
                onClick={toggle}
                aria-expanded={open}
                className="inline-flex h-8 items-center justify-center gap-1.5 rounded-full bg-transparent px-3 text-sm font-medium text-text-secondary hover:bg-bg-hover-secondary hover:text-text-primary transition-colors"
              >
                {selectedModel.name}
                <FiChevronDown size={12} className={`shrink-0 transition-transform ${open ? "rotate-180" : ""}`} />
              </button>
            )}
          />

          <button
            type="button"
            aria-label="Voice input"
            onClick={() => console.log("mic")}
            className="flex h-8 w-8 shrink-0 items-center justify-center rounded-full text-text-secondary hover:bg-bg-hover-primary hover:text-text-primary transition-colors"
          >
            <LuMic size={16} />
          </button>

          <button
            type="button"
            aria-label="Send"
            disabled={empty}
            onClick={() => {
              if (empty) return;
              onSubmit();
            }}
            className="flex h-8 w-8 shrink-0 items-center justify-center rounded-full bg-bg-hover-secondary text-text-secondary transition-colors hover:bg-bg-hover-primary focus:outline-none focus-visible:bg-bg-hover-primary disabled:bg-bg-hover-secondary disabled:text-text-secondary enabled:bg-white enabled:text-bg-primary enabled:hover:opacity-90"
          >
            <HiArrowUp size={16} />
          </button>
        </div>
      </div>


    </div>
  );
}
