import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open } from "@tauri-apps/plugin-dialog";
import { useEffect, useRef, useState } from "react";
import { FiChevronDown } from "react-icons/fi";
import { HiArrowUp, HiPlus, HiStop } from "react-icons/hi";
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
import { slashList, type ChatModel, type SlashCmd } from "../lib/ipc";
import { attachStore, useAttachments } from "../stores/attachments";
import AttachChips from "./AttachChips";
import Dropdown, { type DropdownItem } from "./Dropdown";
import LibraryPanel from "./LibraryPanel";
import SlashMenu from "./SlashMenu";

const COLLAPSED_HEIGHT = 40;
const MAX_HEIGHT = 240;

/// A whole line that is a command: the name, then anything after it.
const SLASH_CALL = /^\/([a-z0-9]+)(?:\s+([\s\S]*))?$/i;

type ChatInputProps = {
  value: string;
  onChange: (next: string) => void;
  onSubmit: () => void;
  /// A line that is a command. Absent means commands are sent as text, which
  /// is what a box with no registry behind it wants.
  onSlash?: (name: string, arg: string) => void;
  placeholder?: string;
  model?: ChatModel | null;
  onModelChange?: (next: ChatModel | null) => void;
  permission?: string;
  onPermissionChange?: (next: string) => void;
  webSearch?: boolean;
  onWebSearchChange?: (next: boolean) => void;
  running?: boolean;
  onStop?: () => void;
  /// Null for a new chat, where there is no session to hang files off yet.
  sessionId?: string | null;
};

const PERM_LABEL: Record<string, string> = {
  never: "Approve for me",
  ask: "Ask always",
};

export default function ChatInput({
  value,
  onChange,
  onSubmit,
  onSlash,
  placeholder = "Work with Argus",
  model,
  onModelChange,
  permission = "ask",
  onPermissionChange,
  webSearch = false,
  onWebSearchChange,
  running = false,
  onStop,
  sessionId = null,
}: ChatInputProps) {
  const textareaRef = useRef<HTMLTextAreaElement | null>(null);
  const { models, loading } = useChatModels();
  const [picked, setPicked] = useState<ChatModel | null>(null);
  const [browsing, setBrowsing] = useState(false);
  const [slashOff, setSlashOff] = useState(false);
  const [dropping, setDropping] = useState(false);
  const [cmds, setCmds] = useState<SlashCmd[]>([]);
  const selected = model ?? picked;

  // The command list lives here, not in the menu, because the box has to know
  // whether the menu has something to offer before it lets go of Enter.
  useEffect(() => {
    let live = true;
    void slashList()
      .then((list) => live && setCmds(list))
      .catch(() => live && setCmds([]));

    return () => {
      live = false;
    };
  }, []);

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

  // A drop carries real paths, same as the dialog does, so both land in the
  // store and the cap and the copy happen in one place.
  useEffect(() => {
    const webview = getCurrentWebview();
    const pending = webview.onDragDropEvent((event) => {
      if (event.payload.type === "over") {
        setDropping(true);
        return;
      }

      setDropping(false);
      if (event.payload.type === "drop") {
        void attachStore.addPaths(event.payload.paths);
      }
    });

    return () => {
      void pending.then((un) => un());
    };
  }, [sessionId]);

  const { items } = useAttachments();

  // Files alone are worth sending: the backend writes the attachment note, so
  // an empty box with chips on it is not an empty turn.
  const empty = value.trim().length === 0 && items.length === 0;

  // A `/` only opens the menu at the start of the line and with no newline
  // after it. That is what stops `src/lib` and a pasted path from being read
  // as a command.
  const asSlash = /^\/([a-z0-9]*)$/i.exec(value);
  const asCall = SLASH_CALL.exec(value);
  const showMenu = asSlash !== null && !running;
  const query = asSlash?.[1] ?? "";
  // What the menu would complete to. Empty means it has nothing, and Enter is
  // then an ordinary send rather than a keystroke that goes nowhere.
  const hit = cmds.find((c) => c.name.startsWith(query));

  const addItems: DropdownItem[] = [
    {
      label: "Add files or photos",
      Icon: RiAttachment2,
      onClick: async () => {
        const pickedPaths = await open({ multiple: true });
        if (pickedPaths === null) {
          return;
        }

        const paths = Array.isArray(pickedPaths) ? pickedPaths : [pickedPaths];
        await attachStore.addPaths(paths);
      },
    },
    {
      label: "Add from library",
      Icon: LuLibrary,
      stayOpen: true,
      onClick: () => setBrowsing(true),
    },
    {
      label: "Add project",
      Icon: LuFolderOpen,
      hasSubmenu: true,
      onClick: () => {},
    },
    {
      label: "Connector",
      Icon: LuPlug,
      hasSubmenu: true,
      onClick: () => {},
    },
    {
      label: "Web search",
      Icon: LuGlobe,
      toggleable: true,
      active: webSearch,
      onClick: () => onWebSearchChange?.(!webSearch),
    },
  ];

  // Completing a command fills the box with its name and leaves the caret
  // after it. The argument stays a hint in the menu: pasting "today | week |
  // month" into the box would send a window called that.
  const accept = (cmd: SlashCmd) => {
    onChange(`/${cmd.name} `);
    textareaRef.current?.focus();
  };

  // The menu can tell the line is already the whole command, which the
  // textarea cannot: it never sees the keystroke the menu took.
  const run = (cmd: SlashCmd) => {
    onChange("");
    onSlash?.(cmd.name, "");
  };

  // A line that is exactly `/name arg` is a command. Anything else — a path,
  // a sentence with a slash in it — is a message.
  const send = () => {
    if (onSlash !== undefined && asCall !== null) {
      onChange("");
      onSlash(asCall[1].toLowerCase(), (asCall[2] ?? "").trim());
      return;
    }

    if (!empty) {
      onSubmit();
    }
  };

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
    <div
      className={
        "flex w-[700px] max-w-full flex-col rounded-2xl border bg-bg-secondary p-3 " +
        (dropping
          ? "border-accent ring-2 ring-accent/30"
          : "border-border-primary")
      }
    >
      <AttachChips />

      <div className="relative">
        <textarea
          ref={textareaRef}
          value={value}
          onChange={(event) => {
            setSlashOff(false);
            onChange(event.target.value);
          }}
          onKeyDown={(event) => {
            // The menu takes Enter and Tab while it has something to complete,
            // and it stops them at the document, so this only sees the rest.
            if (event.key !== "Enter" || event.shiftKey || hit !== undefined) {
              return;
            }

            event.preventDefault();
            send();
          }}
          placeholder={placeholder}
          rows={1}
          style={{ height: COLLAPSED_HEIGHT, lineHeight: "20px" }}
          className="min-h-[44px] w-full resize-none bg-transparent px-2 text-sm font-medium text-text-primary placeholder:font-normal placeholder:text-text-secondary focus:outline-none"
        />

        {showMenu && !slashOff && (
          <SlashMenu
            query={query}
            cmds={cmds}
            onPick={accept}
            onRun={run}
            onClose={() => setSlashOff(true)}
          />
        )}
      </div>

      <div className="mt-2 flex items-center justify-between gap-2">
        <div className="flex items-center gap-2">
          <Dropdown
            items={addItems}
            side="top"
            align="left"
            panel={
              browsing ? (
                <LibraryPanel onBack={() => setBrowsing(false)} />
              ) : undefined
            }
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
            onClick={() => {}}
            className="flex h-8 w-8 shrink-0 items-center justify-center rounded-full text-text-secondary transition-colors hover:bg-bg-hover-primary hover:text-text-primary"
          >
            <LuMic size={16} />
          </button>

          <button
            type="button"
            aria-label={running ? "Stop" : "Send"}
            disabled={!running && empty}
            onClick={() => {
              if (running) {
                onStop?.();
                return;
              }

              send();
            }}
            className={
              "flex h-8 w-8 shrink-0 items-center justify-center rounded-full " +
              "transition-colors focus:outline-none focus-visible:bg-bg-hover-primary " +
              `${
                running
                  ? "bg-text-primary text-bg-primary hover:opacity-90"
                  : "bg-bg-hover-secondary text-text-secondary hover:bg-bg-hover-primary " +
                    "disabled:bg-bg-hover-secondary disabled:text-text-tertiary " +
                    "enabled:bg-accent enabled:text-bg-primary enabled:hover:opacity-90"
              }`
            }
          >
            {running ? <HiStop size={14} /> : <HiArrowUp size={16} />}
          </button>
        </div>
      </div>
    </div>
  );
}
