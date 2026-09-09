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

import { useChatModels } from "../hooks/useChatModels";
import type { ChatModel } from "../lib/ipc";
import Dropdown, { type DropdownItem } from "./Dropdown";

const COLLAPSED_HEIGHT = 40;
const MAX_HEIGHT = 240;

type ChatInputProps = {
  value: string;
  onChange: (next: string) => void;
  onSubmit: () => void;
  placeholder?: string;
  model?: ChatModel | null;
  onModelChange?: (next: ChatModel | null) => void;
  permission?: string;
  onPermissionChange?: (next: string) => void;
  webSearch?: boolean;
  onWebSearchChange?: (next: boolean) => void;
};

const PERM_LABEL: Record<string, string> = {
  never: "Approve for me",
  ask: "Ask always",
};

export default function ChatInput({
  value,
  onChange,
  onSubmit,
  placeholder = "Work with Argus",
  model,
  onModelChange,
  permission = "ask",
  onPermissionChange,
  webSearch = false,
  onWebSearchChange,
}: ChatInputProps) {
  const textareaRef = useRef<HTMLTextAreaElement | null>(null);
  const { models, loading } = useChatModels();
  const [picked, setPicked] = useState<ChatModel | null>(null);
  const selected = model ?? picked;

  useEffect(() => {
    const textarea = textareaRef.current;
    if (!textarea) {
      return;
    }

    textarea.style.height = "auto";
    const next = Math.min(textarea.scrollHeight, MAX_HEIGHT);
    textarea.style.height = `${next}px`;
    textarea.style.overflowY =
      textarea.scrollHeight > MAX_HEIGHT ? "auto" : "hidden";
  }, [value]);

  const empty = value.trim().length === 0;

  const addItems: DropdownItem[] = [
    {
      label: "Add files or photos",
      Icon: RiAttachment2,
      onClick: () => console.log("attach"),
    },
    {
      label: "Add from library",
      Icon: LuLibrary,
      hasSubmenu: true,
      onClick: () => console.log("library"),
    },
    {
      label: "Add project",
      Icon: LuFolderOpen,
      hasSubmenu: true,
      onClick: () => console.log("project"),
    },
    {
      label: "Connector",
      Icon: LuPlug,
      hasSubmenu: true,
      onClick: () => console.log("connector"),
    },
    {
      label: "Web search",
      Icon: LuGlobe,
      toggleable: true,
      active: webSearch,
      onClick: () => onWebSearchChange?.(!webSearch),
    },
  ];

  const permissionItems: DropdownItem[] = [
    {
      label: PERM_LABEL.never,
      onClick: () => onPermissionChange?.("never"),
      active: permission === "never",
    },
    {
      label: PERM_LABEL.ask,
      onClick: () => onPermissionChange?.("ask"),
      active: permission === "ask",
    },
  ];

  const autoItem: DropdownItem = {
    label: "Auto",
    onClick: () => {
      setPicked(null);
      onModelChange?.(null);
    },
    active: selected === null,
  };

  const modelItems: DropdownItem[] =
    models.length === 0
      ? [
          autoItem,
          {
            label: loading ? "Loading models…" : "No models enabled",
            disabled: true,
          },
        ]
      : [
          autoItem,
          ...models.map((m) => ({
            label: m.displayName,
            onClick: () => {
              setPicked(m);
              onModelChange?.(m);
            },
            active: selected?.modelId === m.modelId,
          })),
        ];

  return (
    <div className="flex w-[700px] max-w-full flex-col rounded-2xl border border-border-primary bg-bg-secondary p-3 shadow-4xl">
      <textarea
        ref={textareaRef}
        value={value}
        onChange={(event) => onChange(event.target.value)}
        onKeyDown={(event) => {
          if (event.key !== "Enter" || event.shiftKey) {
            return;
          }

          event.preventDefault();
          if (!empty) {
            onSubmit();
          }
        }}
        placeholder={placeholder}
        rows={1}
        style={{ height: COLLAPSED_HEIGHT, lineHeight: "20px" }}
        className="min-h-[44px] w-full resize-none bg-transparent px-2 text-sm font-medium text-text-primary placeholder:font-normal placeholder:text-text-secondary focus:outline-none"
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
                className="inline-flex h-8 items-center justify-center gap-1.5 rounded-full bg-transparent px-3 text-sm font-medium text-text-secondary transition-colors hover:bg-bg-hover-secondary hover:text-text-primary"
              >
                <LuShieldCheck size={14} className="shrink-0" />
                {PERM_LABEL[permission] ?? permission}
                <FiChevronDown
                  size={12}
                  className={`shrink-0 transition-transform ${open ? "rotate-180" : ""}`}
                />
              </button>
            )}
          />
        </div>

        <div className="flex items-center gap-2">
          <Dropdown
            items={modelItems}
            side="top"
            align="right"
            dividers={false}
            maxH="320px"
            header={
              <span className="text-xs font-medium text-text-secondary">
                Models
              </span>
            }
            trigger={({ open, toggle }) => (
              <button
                type="button"
                onClick={toggle}
                aria-expanded={open}
                className="inline-flex h-8 items-center justify-center gap-1.5 rounded-full bg-transparent px-3 text-sm font-medium text-text-secondary transition-colors hover:bg-bg-hover-secondary hover:text-text-primary"
              >
                {selected?.displayName ?? "Auto"}
                <FiChevronDown
                  size={12}
                  className={`shrink-0 transition-transform ${open ? "rotate-180" : ""}`}
                />
              </button>
            )}
          />

          <button
            type="button"
            aria-label="Voice input"
            onClick={() => console.log("mic")}
            className="flex h-8 w-8 shrink-0 items-center justify-center rounded-full text-text-secondary transition-colors hover:bg-bg-hover-primary hover:text-text-primary"
          >
            <LuMic size={16} />
          </button>

          <button
            type="button"
            aria-label="Send"
            disabled={empty}
            onClick={() => {
              if (empty) {
                return;
              }

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
