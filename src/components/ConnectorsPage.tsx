import {
  LuCalendar,
  LuDatabase,
  LuGithub,
  LuHardDrive,
  LuMail,
  LuMessageSquare,
  LuSearch,
} from "react-icons/lu";

import ConnectorCard, { type Connector } from "./ConnectorCard";

const CONNECTORS: Connector[] = [
  {
    id: "gmail",
    name: "Gmail",
    description: "Connect your Gmail inbox",
    icon: <LuMail />,
  },
  {
    id: "github",
    name: "GitHub",
    description: "Sync repositories & PRs",
    icon: <LuGithub />,
  },
  {
    id: "calendar",
    name: "Google Calendar",
    description: "Manage events & meetings",
    icon: <LuCalendar />,
  },
  {
    id: "drive",
    name: "Google Drive",
    description: "Access docs & files",
    icon: <LuHardDrive />,
  },
  {
    id: "slack",
    name: "Slack",
    description: "Collaborate with your team",
    icon: <LuMessageSquare />,
  },
  {
    id: "notion",
    name: "Notion",
    description: "Sync notes & databases",
    icon: <LuDatabase />,
  },
];

export default function ConnectorsPage() {
  return (
    <div className="mx-auto w-full max-w-2xl py-4">
      <h1 className="mb-1 text-2xl font-medium text-text-primary">
        Connectors
      </h1>
      <p className="mb-5 text-sm font-medium text-text-secondary">
        Connect Argus to your favourite tools.
      </p>
      <div
        className={
          "flex h-10 w-full items-center rounded-full border " +
          "border-border-primary bg-bg-secondary px-4"
        }
      >
        <LuSearch size={18} className="shrink-0 text-text-secondary" />
        <input
          type="text"
          placeholder="Search connectors..."
          className={
            "ml-2 flex-1 bg-transparent text-sm text-text-primary " +
            "outline-none placeholder:text-text-secondary"
          }
        />
      </div>
      <div className="mt-6 grid grid-cols-2 gap-4">
        {CONNECTORS.map((connector) => (
          <ConnectorCard key={connector.id} connector={connector} />
        ))}
      </div>
    </div>
  );
}
