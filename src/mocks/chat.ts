export type ChatRole = "user" | "agent";

export type ChatMessage = {
  id: string;
  role: ChatRole;
  content: string;
  timestamp?: Date | string | number;
};

const minutesAgo = (m: number) => new Date(Date.now() - m * 60_000);

export const MOCK_MESSAGES: ChatMessage[] = [
  {
    id: "m1",
    role: "user",
    content: "Help me refactor the auth flow — the current one is too tangled.",
    timestamp: minutesAgo(18),
  },
  {
    id: "m2",
    role: "agent",
    content: "Sure. Let me start by mapping the current call graph so I can spot the worst <bold>coupling</bold> before I propose anything.",
  },
  {
    id: "m3",
    role: "user",
    content: "Also the Tauri build keeps failing on second run, can you look at that too?",
    timestamp: minutesAgo(5),
  },
  {
    id: "m4",
    role: "agent",
    content: `I'll clean up the auth module and summarize the changes.

<h3>Overview</h3>
This is a normal paragraph showing the base <bold>bold</bold>, <italic>italic</italic>, <underline>underline</underline>, <strikethrough>strikethrough</strikethrough> and <code>inline code</code> styles.

Links are now blue and clickable: <link href="https://tauri.app">Tauri docs</link> and <link href="https://example.com">example.com</link> — raw https://github.com/anyscape/auth also works.

<h3>Details</h3>
Another paragraph with mixed formatting — <bold>bold and <italic>nested italic</italic> inside</bold> — to verify nesting and line height.

- First bullet with <bold>medium bold</bold> and <code>code</code>
- Second bullet with a link: <link href="https://example.com">example.com</link> and raw https://tauri.app
- Third bullet plain text to check spacing

Numbered steps:
1. First step with mention @alex and @byron
2. Second step with <code>cargo test</code> and link https://docs.rs
3. Third step plain

Mentions:
Hey @alex, can you review this? cc @byron — tools: @gmail and @chrome should show chrome icon.

Connector roundtrip:
- Pulled the latest invoice from @gmail and opened the link in @chrome
- Drafted the reply in @docs and shared it to @drive
- Pushed the branch via @github and scheduled a review on @calendar
- Joined the standup in @meet and dropped notes in @sheets

<table>
<tr><th>Name</th><th>Status</th><th>Updated</th></tr>
<tr><td>Auth module</td><td>Done</td><td>2h ago</td></tr>
<tr><td>Tauri build</td><td>Running</td><td>now</td></tr>
<tr><td>API spec</td><td>Pending</td><td>—</td></tr>
</table>

Attachments:
Image preview (not in Files Edited):
<file path="https://picsum.photos/seed/cover/600/400" action="created" type="image" />

Doc via document tag (not file):
<document path="public/spec.pdf" title="Spec" doctype="pdf" pages="12" status="ready" />

Files Edited (hover + always show diffs — new file shows +140):
<file path="src/auth.rs" action="edited" type="code" />
<file path="src/new_feature.rs" action="created" type="code" />`,
  },
];
