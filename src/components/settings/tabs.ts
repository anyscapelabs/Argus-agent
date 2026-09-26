import {
  LuBot,
  LuBrain,
  LuPalette,
  LuServer,
  LuShieldCheck,
  LuTerminal,
  LuUsers,
} from "react-icons/lu";

export type SettingsTab =
  | "models"
  | "providers"
  | "terminal"
  | "sandbox"
  | "agents"
  | "profiles"
  | "theme";

export type TabIcon = React.ComponentType<{ size?: number; className?: string }>;

/// Weakest first, roughly by how much of the machine each one reaches. The
/// order is the order the user reads it in, so it should not jump around.
export const SETTINGS_TABS: {
  tab: SettingsTab;
  label: string;
  group: string;
  Icon: TabIcon;
}[] = [
  { tab: "models", label: "Models", group: "AI", Icon: LuBrain },
  { tab: "providers", label: "Providers", group: "AI", Icon: LuServer },
  { tab: "agents", label: "Sub-agents", group: "AI", Icon: LuBot },
  { tab: "terminal", label: "Terminal", group: "Permissions", Icon: LuTerminal },
  { tab: "sandbox", label: "Sandbox", group: "Permissions", Icon: LuShieldCheck },
  { tab: "profiles", label: "Profiles", group: "Personalization", Icon: LuUsers },
  { tab: "theme", label: "Appearance", group: "Personalization", Icon: LuPalette },
];

export const SETTINGS_TITLE: Record<SettingsTab, string> = {
  models: "Models",
  providers: "Providers",
  terminal: "Terminal",
  sandbox: "Sandbox",
  agents: "Sub-agents",
  profiles: "Profiles",
  theme: "Appearance",
};

/// One line under the title, said once by the page shell. Pages used to carry
/// their own heading as well, so every one of them printed its name twice.
export const SETTINGS_DESC: Record<SettingsTab, string> = {
  models: "Which models you can reach and what each one is for.",
  providers: "Manage your model providers and configure API keys.",
  terminal: "The shell Argus runs commands in.",
  sandbox:
    "Isolated commands run under a boundary the OS enforces directly. If a profile cannot be enforced, the command is refused — never run unconfined.",
  agents:
    "Each sub-agent keeps its own transcript. Nothing else ever reads it, so it is safe to let old ones go.",
  profiles:
    "A profile is a name, a set of instructions, and what it may reach. It owns its chats and the sub-agents inside them, and nothing else.",
  theme: "Argus repaints immediately. Nothing reloads and no work is lost.",
};
