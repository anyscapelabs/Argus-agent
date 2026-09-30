// The typed edge to the backend. Every `#[tauri::command]` in Rust has exactly
// one wrapper here, and nothing else in the app calls `invoke` directly.
//
// Split by domain so a wrapper is found next to its siblings: `sessions.ts`
// holds every `sess_*` call, `connectors.ts` every OAuth one. The command names
// themselves live in `commands.ts`; a rename on the Rust side is a one-line
// change there.
//
// Re-exported wholesale, so `import { sessListSessions } from "../lib/ipc"`
// keeps working exactly as before.

export * from "./agents";
export * from "./commands";
export * from "./connectors";
export * from "./errors";
export * from "./gateway";
export * from "./jobs";
export * from "./library";
export * from "./memory";
export * from "./profiles";
export * from "./sandbox";
export * from "./sessions";
export * from "./skills";
export * from "./slash";
export * from "./stream";
export * from "./terminal";
export * from "./voice";
