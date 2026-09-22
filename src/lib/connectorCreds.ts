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
  details?: string;
};

export const CONNECTOR_SERVICES: ConnectorService[] = [
  {
    id: "linear",
    name: "Linear",
    tagline: "Issues, cycles and comments",
    details:
      "Fetch issues and teams, create issues, and comment — via your Linear API key.",
    fields: [
      { kind: "token", label: "API key", placeholder: "lin_api_…" },
    ],
  },
  {
    id: "slack",
    name: "Slack",
    tagline: "Channels, threads and DMs",
    details:
      "Read channels, history and threads, list users, and send messages as your bot.",
    fields: [
      { kind: "token", label: "Bot token", placeholder: "xoxb-…" },
    ],
  },
  {
    id: "notion",
    name: "Notion",
    tagline: "Pages, databases and blocks",
    details:
      "Search pages and databases, read content, create pages and append text.",
    fields: [
      { kind: "token", label: "Integration token", placeholder: "ntn_…" },
    ],
  },
  {
    id: "figma",
    name: "Figma",
    tagline: "Files, frames and comments",
    details:
      "Read file structure and comments, and post comments as you.",
    fields: [
      { kind: "token", label: "Personal access token", placeholder: "figd_…" },
    ],
  },
  {
    id: "discord",
    name: "Discord",
    tagline: "Servers, channels and messages",
    details:
      "List servers and text channels, read message history, and send messages as your bot.",
    fields: [
      { kind: "token", label: "Bot token", placeholder: "MTIz…" },
    ],
  },
  {
    id: "exa",
    name: "Exa",
    tagline: "AI web search and page text",
    details:
      "Search the web with clean snippets and fetch full page text — via your Exa API key.",
    fields: [
      { kind: "token", label: "API key", placeholder: "…" },
    ],
  },
  {
    id: "telegram",
    name: "Telegram",
    tagline: "Chats via your bot",
    details:
      "Receive updates sent to your bot and send messages to any chat it can reach.",
    fields: [
      { kind: "token", label: "Bot token", placeholder: "123456:ABC-…" },
    ],
  },
  {
    id: "todoist",
    name: "Todoist",
    tagline: "Tasks and projects",
    details:
      "List tasks with filters, create tasks, and complete or delete them.",
    fields: [
      { kind: "token", label: "API token", placeholder: "…" },
    ],
  },
  {
    id: "gitlab",
    name: "GitLab",
    tagline: "Projects, issues and pipelines",
    details:
      "Browse projects, issues, merge requests and pipelines, and create issues.",
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
    details:
      "Read entity states and the instance config, and call services to control your home.",
    fields: [
      { kind: "token", label: "Long-lived access token", placeholder: "…" },
    ],
  },
  {
    id: "trello",
    name: "Trello",
    tagline: "Boards, lists and cards",
    details:
      "Browse boards, lists and cards, create cards, and comment.",
    fields: [
      { kind: "client", label: "API key", placeholder: "…" },
      { kind: "token", label: "Token", placeholder: "…" },
    ],
  },
  {
    id: "google",
    name: "Google",
    tagline: "Gmail, Calendar, Drive, Docs, Sheets",
    details:
      "Search and read Gmail, send email, manage Calendar events, and work with Drive files, Docs and Sheets.",
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
    tagline: "Repos, issues, PRs and Actions",
    details:
      "Browse repositories, branches, issues and pull requests; create issues and PRs, merge, and re-run failed Actions.",
    fields: [
      { kind: "client", label: "OAuth app client ID", placeholder: "…" },
      { kind: "secret", label: "Client secret", placeholder: "…" },
    ],
  },
  {
    id: "outlook",
    name: "Outlook",
    tagline: "Mail and calendar",
    details:
      "Read your Outlook mail, send email, and manage calendar events.",
    fields: [
      { kind: "client", label: "Application (client) ID", placeholder: "…" },
    ],
  },
  {
    id: "spotify",
    name: "Spotify",
    tagline: "Now playing and playback",
    details:
      "See the currently playing track and control playback: play, pause, skip.",
    fields: [
      { kind: "client", label: "Client ID", placeholder: "…" },
    ],
  },
];
