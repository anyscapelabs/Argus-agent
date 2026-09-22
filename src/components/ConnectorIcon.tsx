import chrome from "../assets/connectors/chrome.svg?raw";
import { LuSearch } from "react-icons/lu";

import discord from "../assets/connectors/discord.svg?raw";
import figma from "../assets/connectors/figma.svg?raw";
import github from "../assets/connectors/github.svg?raw";
import gitlab from "../assets/connectors/gitlab.svg?raw";
import google from "../assets/connectors/google.svg?raw";
import homeassistant from "../assets/connectors/homeassistant.svg?raw";
import linear from "../assets/connectors/linear.svg?raw";
import notion from "../assets/connectors/notion.svg?raw";
import slack from "../assets/connectors/slack.svg?raw";
import spotify from "../assets/connectors/spotify.svg?raw";
import telegram from "../assets/connectors/telegram.svg?raw";
import todoist from "../assets/connectors/todoist.svg?raw";
import trello from "../assets/connectors/trello.svg?raw";

const ICONS: Record<string, string> = {
  chrome,
  discord,
  figma,
  github,
  gitlab,
  google,
  ha: homeassistant,
  linear,
  notion,
  slack,
  spotify,
  telegram,
  todoist,
  trello,
};

type Props = {
  id: string;
  size?: number;
  className?: string;
};

export default function ConnectorIcon({ id, size = 20, className }: Props) {
  const svg = ICONS[id];

  if (!svg) {
    return (
      <span
        className={"inline-flex shrink-0 items-center justify-center text-text-secondary " + (className ?? "")}
        style={{ width: size, height: size }}
      >
        <LuSearch size={Math.round(size * 0.85)} />
      </span>
    );
  }

  return (
    <span
      className={"inline-flex shrink-0 items-center justify-center " + (className ?? "")}
      style={{ width: size, height: size }}
      dangerouslySetInnerHTML={{
        __html: svg.replace(
          "<svg ",
          `<svg width="${size}" height="${size}" style="display:block" `,
        ),
      }}
    />
  );
}
