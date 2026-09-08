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

const connectors: Connector[] = [
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
      <h1 className="text-2xl text-text-primary font-medium mb-1">Connectors</h1>
      <p className="text-sm text-text-secondary font-medium mb-5">
        Connect Argus to your favourite tools.
      </p>

      <div className="flex items-center w-full h-10 px-4 bg-bg-secondary border border-border-primary rounded-full">
        <LuSearch size={18} className="text-text-secondary shrink-0" />
        <input
          type="text"
          placeholder="Search connectors..."
          className="flex-1 ml-2 bg-transparent outline-none text-sm text-text-primary placeholder:text-text-secondary"
        />
      </div>

      <div className="mt-6 grid grid-cols-2 gap-4">
        {connectors.map((connector) => (
          <ConnectorCard key={connector.id} connector={connector} />
        ))}
      </div>
    </div>
  );
}
