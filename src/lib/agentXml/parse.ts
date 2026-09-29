// The entry point: normalize invisibles, tokenize, build the tree, and cache
// the result. The cache exists because the chat re-renders the same finished
// reply on every keystroke elsewhere in the view.
import { buildTree } from "./tree";
import type { XmlTree } from "./tree";
import { normalizeMd } from "./markdown";
import { tokenize } from "./tokenize";

const INVISIBLE_RE =
  /[\u200b\u200c\u200d\ufeff\u00ad\u200e\u200f\u202a-\u202e\u2060-\u206f]/g;
const PARSE_CAP = 1_000_000;

export function parse(buf: string): XmlTree {
  // Belt and braces: Rust strips these before storage, but streamed text
  // reaches here first. Nothing a model writes legitimately needs them.
  const clean = buf.replace(INVISIBLE_RE, "");
  const src = clean.length > PARSE_CAP ? clean.slice(0, PARSE_CAP) : clean;

  try {
    return buildTree(tokenize(normalizeMd(src)));
  } catch {
    return [
      { kind: "paragraph", tag: "p", attrs: {}, children: [{ kind: "text", value: src }] },
    ];
  }
}

const PARSE_CACHE = new Map<string, XmlTree>();

export function parseCached(text: string): XmlTree {
  const hit = PARSE_CACHE.get(text);

  if (hit !== undefined) return hit;

  const tree = parse(text);
  PARSE_CACHE.set(text, tree);

  if (PARSE_CACHE.size > 50) {
    const first = PARSE_CACHE.keys().next();
    if (!first.done) PARSE_CACHE.delete(first.value);
  }

  return tree;
}
