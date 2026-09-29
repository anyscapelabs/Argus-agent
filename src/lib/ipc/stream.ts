// The streaming calls: the chat channel, the event watcher, and the two ways
// to stop one.
import { Channel, invoke } from "@tauri-apps/api/core";

import { CMD_SESS_STREAM } from "./commands";
import type { Attachment } from "./library";
import type { StreamEvent } from "./sessions";

export function sessChatStream(
  sessionId: string,
  content: string,
  onEvent: Channel<StreamEvent>,
  attachments?: Attachment[],
): Promise<void> {
  return invoke<void>(CMD_SESS_STREAM, {
    sessionId,
    content,
    onEvent,
    attachments: attachments ? JSON.stringify(attachments) : null,
  });
}

export function sessWatchEvents(
  sessionId: string,
  onEvent: Channel<StreamEvent>,
): Promise<void> {
  return invoke<void>("sess_watch_events", { sessionId, onEvent });
}
export function sessUnwatch(sessionId?: string): Promise<void> {
  return invoke<void>("sess_unwatch", { sessionId: sessionId ?? null });
}

export function sessCancelChat(sessionId: string): Promise<boolean> {
  return invoke<boolean>("sess_cancel_chat", { sessionId });
}
