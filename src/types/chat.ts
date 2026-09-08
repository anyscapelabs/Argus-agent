export type ChatRole = "user" | "agent" | "tool";

export type ChatMessage = {
  id: string;
  role: ChatRole;
  content: string;
  timestamp?: Date | string | number;
};
