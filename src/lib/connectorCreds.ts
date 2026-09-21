export type FieldKind = "token" | "client" | "secret";

export type FieldDef = {
  kind: FieldKind;
  label: string;
  placeholder: string;
};

export type ConnectorService = {
  id: string;
  name: string;
  fields: FieldDef[];
  tagline?: string;
};

export const CONNECTOR_SERVICES: ConnectorService[] = [
  {
    id: "linear",
    name: "Linear",
    tagline: "Issues, cycles and comments",
    fields: [
      { kind: "token", label: "API key", placeholder: "lin_api_…" },
    ],
  },
  {
    id: "slack",
    name: "Slack",
    tagline: "Channels, threads and DMs",
    fields: [
      { kind: "token", label: "Bot token", placeholder: "xoxb-…" },
    ],
  },
  {
    id: "notion",
    name: "Notion",
    tagline: "Pages, databases and blocks",
    fields: [
      { kind: "token", label: "Integration token", placeholder: "ntn_…" },
    ],
  },
  {
    id: "figma",
    name: "Figma",
    tagline: "Files, frames and comments",
    fields: [
      { kind: "token", label: "Personal access token", placeholder: "figd_…" },
    ],
  },
  {
    id: "discord",
    name: "Discord",
    tagline: "Servers, channels and messages",
    fields: [
      { kind: "token", label: "Bot token", placeholder: "MTIz…" },
    ],
  },
  {
    id: "telegram",
    name: "Telegram",
    tagline: "Chats via your bot",
    fields: [
      { kind: "token", label: "Bot token", placeholder: "123456:ABC-…" },
    ],
  },
  {
    id: "todoist",
    name: "Todoist",
    tagline: "Tasks and projects",
    fields: [
      { kind: "token", label: "API token", placeholder: "…" },
    ],
  },
  {
    id: "gitlab",
    name: "GitLab",
    tagline: "Projects, issues and pipelines",
    fields: [
      {
        kind: "token",
        label: "Personal access token",
        placeholder: "glpat-…",
      },
    ],
  },
  {
    id: "ha",
    name: "Home Assistant",
    tagline: "Entities, states and services",
    fields: [
      { kind: "token", label: "Long-lived access token", placeholder: "…" },
    ],
  },
  {
    id: "trello",
    name: "Trello",
    tagline: "Boards, lists and cards",
    fields: [
      { kind: "client", label: "API key", placeholder: "…" },
      { kind: "token", label: "Token", placeholder: "…" },
    ],
  },
  {
    id: "google",
    name: "Google",
    fields: [
      {
        kind: "client",
        label: "OAuth client ID",
        placeholder: "…apps.googleusercontent.com",
      },
      { kind: "secret", label: "OAuth client secret", placeholder: "GOCSPX-…" },
    ],
  },
  {
    id: "github",
    name: "GitHub",
    fields: [
      { kind: "client", label: "OAuth app client ID", placeholder: "…" },
      { kind: "secret", label: "Client secret", placeholder: "…" },
    ],
  },
  {
    id: "outlook",
    name: "Outlook",
    fields: [
      { kind: "client", label: "Application (client) ID", placeholder: "…" },
    ],
  },
  {
    id: "spotify",
    name: "Spotify",
    fields: [
      { kind: "client", label: "Client ID", placeholder: "…" },
    ],
  },
];
