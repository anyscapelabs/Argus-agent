// The shell the terminal panel runs in.
import { invoke } from "@tauri-apps/api/core";

import {
  CMD_TERM_CLEAR,
  CMD_TERM_SET,
  CMD_TERM_STATUS,
} from "./commands";


export type TermShellStatus = {
  binary: string;
  kind: string;
  version: string | null;
  source: string;
};

export function termShellStatus(): Promise<TermShellStatus> {
  return invoke<TermShellStatus>(CMD_TERM_STATUS);
}

export function termShellSet(path: string): Promise<TermShellStatus> {
  return invoke<TermShellStatus>(CMD_TERM_SET, { path });
}

export function termShellClear(): Promise<TermShellStatus> {
  return invoke<TermShellStatus>(CMD_TERM_CLEAR);
}
