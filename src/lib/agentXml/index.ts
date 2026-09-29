// Reading the agent's own markup out of a reply: schema, tokenizer, tree, and
// renderer, one stage per file.
//
// The stages are strictly one-way — nothing downstream reaches back up — which
// is what makes a reply safe to render: an unrecognised tag is a text node, not
// a crash, because only `schema.ts` decides what a tag means and it is allowed
// to say "no".
//
// Re-exported wholesale, so `import { parse, TAG_SCHEMA } from "./agentXml"`
// keeps working exactly as before.

export * from "./markdown";
export * from "./parse";
export * from "./schema";
export * from "./tokenize";
export * from "./tree";
