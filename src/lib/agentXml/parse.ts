// Cached: the chat re-renders the same finished reply on every keystroke.
import { buildTree } from "./tree";
import type { XmlTree } from "./tree";
import { normalizeMd } from "./markdown";
import { tokenize } from "./tokenize";

const PARSE_CAP = 1_000_000;

/**
 * Invisible code points: zero-width joiners, soft hyphens, bidi overrides, the
 * word-joiner block. A model emits one mid-tag and the tag never closes.
 *
 * A scan, not a global regex: the string is walked once and returned untouched
 * in the common case, and this runs on every delta of every turn.
 */
const INVISIBLE = new Set([
  0x200b, 0x200c, 0x200d, 0xfeff, 0x00ad, 0x200e, 0x200f, 0x202a, 0x202b,
  0x202c, 0x202d, 0x202e, 0x2060, 0x2061, 0x2062, 0x2063, 0x2064, 0x2065,
  0x2066, 0x2067, 0x2068, 0x2069, 0x206a, 0x206b, 0x206c, 0x206d, 0x206e,
  0x206f,
]);

function stripInvisible(s: string): string {
  const first = findInvisible(s, 0);
  if (first === -1) return s;

  let out = s.slice(0, first);
  for (let i = first; i < s.length; i++) {
    const code = s.charCodeAt(i);
    if (INVISIBLE.has(code)) continue;
    out += s[i];
  }
  return out;
}

function findInvisible(s: string, from: number): number {
  for (let i = from; i < s.length; i++) {
    if (INVISIBLE.has(s.charCodeAt(i))) return i;
  }
  return -1;
}

/**
 * `final` says whether more text can still arrive. See `TokenizeOpts`.
 *
 * A live turn passes `false` so an unfinished tag is held back instead of
 * flashing as raw markup; everything from the database passes the default.
 */
export function parse(buf: string, opts: { final?: boolean } = {}): XmlTree {
  const final = opts.final !== false;
  // Belt and braces: Rust strips these before storage, but streamed text
  // reaches here first.
  const clean = stripInvisible(buf);
  const src = clean.length > PARSE_CAP ? clean.slice(0, PARSE_CAP) : clean;

  try {
    return buildTree(tokenize(normalizeMd(src, { final }), { final }));
  } catch {
    return [
      {
        kind: "paragraph",
        tag: "p",
        attrs: {},
        children: [{ kind: "text", value: src }],
      },
    ];
  }
}

const PARSE_CACHE = new Map<string, XmlTree>();

/** The cache is keyed by finality too: a live buffer and a settled one parse differently. */
const cacheKey = (text: string, final: boolean) =>
  final ? `final:${text}` : `live:${text}`;

export function parseCached(
  text: string,
  opts: { final?: boolean } = {},
): XmlTree {
  const final = opts.final !== false;
  const key = cacheKey(text, final);
  const hit = PARSE_CACHE.get(key);

  if (hit !== undefined) return hit;

  const tree = parse(text, { final });
  PARSE_CACHE.set(key, tree);

  if (PARSE_CACHE.size > 50) {
    const first = PARSE_CACHE.keys().next();
    if (!first.done) PARSE_CACHE.delete(first.value);
  }

  return tree;
}
