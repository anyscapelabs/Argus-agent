import { IoSparklesOutline } from "react-icons/io5";
import {
  LuPalette,
  LuServer,
  LuShield,
  LuTerminal,
  LuUsers,
} from "react-icons/lu";
import { TbUser } from "react-icons/tb";

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
export const SETTINGS_TABS: { tab: SettingsTab; label: string; Icon: TabIcon }[] = [
  { tab: "models", label: "Models", Icon: IoSparklesOutline },
  { tab: "providers", label: "Providers", Icon: LuServer },
  { tab: "terminal", label: "Terminal", Icon: LuTerminal },
  { tab: "sandbox", label: "Sandbox", Icon: LuShield },
  { tab: "agents", label: "Sub-agents", Icon: LuUsers },
  { tab: "profiles", label: "Profiles", Icon: TbUser },
  { tab: "theme", label: "Appearance", Icon: LuPalette },
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
