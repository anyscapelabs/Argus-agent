// What tags mean. The schema is the contract the prompt is built from, so a
// tag added here is a tag the model is told about and the work panel knows how
// to file.
export type TagSchema = {
  tag: string;
  selfClosing: boolean;
  attributes: {
    name: string;
    required?: boolean;
    values?: readonly string[];
  }[];
};

export const TAG_SCHEMA: readonly TagSchema[] = [
  { tag: "bold", selfClosing: false, attributes: [] },
  { tag: "italic", selfClosing: false, attributes: [] },
  { tag: "underline", selfClosing: false, attributes: [] },
  { tag: "strikethrough", selfClosing: false, attributes: [] },
  { tag: "code", selfClosing: false, attributes: [] },
  { tag: "codeblock", selfClosing: false, attributes: [{ name: "language" }] },
  { tag: "link", selfClosing: false, attributes: [{ name: "href" }] },
  { tag: "h1", selfClosing: false, attributes: [] },
  { tag: "h2", selfClosing: false, attributes: [] },
  { tag: "h3", selfClosing: false, attributes: [] },
  { tag: "h4", selfClosing: false, attributes: [] },
  { tag: "table", selfClosing: false, attributes: [] },
  { tag: "tr", selfClosing: false, attributes: [] },
  { tag: "th", selfClosing: false, attributes: [] },
  { tag: "td", selfClosing: false, attributes: [] },
  {
    tag: "agent",
    selfClosing: false,
    attributes: [{ name: "id" }, { name: "name" }, { name: "state" }],
  },
  {
    tag: "agent-done",
    selfClosing: false,
    attributes: [{ name: "id" }, { name: "name" }, { name: "state" }],
  },
  { tag: "thinking", selfClosing: false, attributes: [{ name: "id" }] },
  { tag: "plan", selfClosing: false, attributes: [{ name: "id" }] },
  {
    tag: "step",
    selfClosing: false,
    attributes: [
      { name: "id" },
      { name: "status", values: ["done", "running", "pending"] },
    ],
  },
  {
    tag: "action",
    selfClosing: false,
    attributes: [
      { name: "id" },
      { name: "tool" },
      { name: "risk", values: ["low", "medium", "high"] },
      { name: "status", values: ["running", "success", "error"] },
    ],
  },
  {
    tag: "approval",
    selfClosing: false,
    attributes: [
      { name: "id" },
      {
        name: "type",
        values: [
          "git_push",
          "payment",
          "send_email",
          "delete",
          "login",
          "force_push",
        ],
      },
    ],
  },
  {
    tag: "diff",
    selfClosing: false,
    attributes: [{ name: "file" }, { name: "language" }],
  },
  {
    tag: "file",
    selfClosing: true,
    attributes: [
      { name: "path" },
      { name: "action", values: ["created", "edited", "read", "deleted"] },
      { name: "type" },
    ],
  },
  {
    tag: "document",
    selfClosing: true,
    attributes: [
      { name: "id" },
      { name: "path" },
      { name: "title" },
      {
        name: "doctype",
        values: ["docx", "pdf", "pptx", "xlsx", "csv", "md", "txt"],
      },
      { name: "pages" },
      { name: "status", values: ["ready", "generating"] },
    ],
  },
  {
    tag: "terminal",
    selfClosing: false,
    attributes: [
      { name: "id" },
      { name: "command" },
      { name: "status", values: ["running", "success", "error"] },
    ],
  },
  {
    tag: "sandbox",
    selfClosing: false,
    attributes: [
      { name: "command" },
      { name: "profile" },
      { name: "origin" },
      { name: "status" },
    ],
  },
  {
    tag: "check",
    selfClosing: true,
    attributes: [{ name: "status", values: ["pass", "retry"] }],
  },
  {
    tag: "email-draft",
    selfClosing: false,
    attributes: [{ name: "id" }, { name: "to" }, { name: "subject" }],
  },
  {
    tag: "browser-action",
    selfClosing: false,
    attributes: [{ name: "id" }, { name: "url" }, { name: "action" }],
  },
  {
    tag: "memory-ref",
    selfClosing: false,
    attributes: [{ name: "source" }, { name: "date" }],
  },
  {
    tag: "warning",
    selfClosing: false,
    attributes: [{ name: "severity", values: ["low", "medium", "high"] }],
  },
  {
    tag: "error",
    selfClosing: false,
    attributes: [{ name: "severity", values: ["low", "medium", "high"] }, { name: "kind" }],
  },
] as const;

const TAG_MAP: Map<string, TagSchema> = new Map(
  TAG_SCHEMA.map((s) => [s.tag, s]),
);

export function isKnownTag(tag: string): boolean {
  return TAG_MAP.has(tag);
}

export function getTagSchema(tag: string): TagSchema | undefined {
  return TAG_MAP.get(tag);
}
