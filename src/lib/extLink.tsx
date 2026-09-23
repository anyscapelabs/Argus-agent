import type { ReactNode } from "react";

import { openUrl } from "@tauri-apps/plugin-opener";

export const EXT_LINK_CLS =
  "cursor-pointer break-all text-blue-400 underline decoration-blue-400/30 " +
  "underline-offset-2 hover:text-blue-300";

export function openExternal(url: string) {
  if (/^https?:\/\//.test(url)) {
    openUrl(url).catch(() => {});
  }
}

export function ExtLink({ href, children }: { href: string; children: ReactNode }) {
  return (
    <a
      href={href}
      rel="noreferrer"
      onClick={(e) => {
        e.preventDefault();
        openExternal(href);
      }}
      className={EXT_LINK_CLS}
    >
      {children}
    </a>
  );
}
