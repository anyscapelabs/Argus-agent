// Character classification, by lookup table rather than by regular
// expression.
//
// Every scanner in this pipeline asks the same few questions about a single
// character: is this a space, can it start a tag name, is it a hex digit? Those
// are `charCodeAt` plus one array read. A regular expression cannot be used
// for them without constructing a match object per call, and it cannot be
// interrupted and resumed, which is the whole problem the streaming scanners
// here have to solve.
//
// The table is built once from ranges rather than written out by hand: a
// mistake in a 128-entry literal is invisible in review, whereas a range is
// self-evidently the set of ASCII letters.

const ASCII_LAST = 0x7f;

/** Bit flags, so one table can answer several questions with one read. */
export const enum Char {
  None = 0,
  Space = 1 << 0,
  TagNameStart = 1 << 1,
  TagName = 1 << 2,
  AttrName = 1 << 3,
  Digit = 1 << 4,
  HexDigit = 1 << 5,
  Alpha = 1 << 6,
  /** Terminates an unquoted attribute value. */
  AttrEnd = 1 << 7,
}

function build(): Uint8Array {
  const t = new Uint8Array(ASCII_LAST + 1);

  const add = (lo: number, hi: number, flag: Char) => {
    for (let c = lo; c <= hi; c++) t[c] |= flag;
  };

  add(0x09, 0x0d, Char.Space); // tab, lf, vt, ff, cr
  add(0x20, 0x20, Char.Space);
  add(0x28, 0x2b, Char.AttrEnd); // ( ) * +
  add(0x2d, 0x2d, Char.TagName | Char.AttrName); // -
  // Digits are name characters, not just value characters: `h2`, `h3` and
  // `email-draft` all have digits in the name.
  add(0x30, 0x39, Char.Digit | Char.HexDigit | Char.TagName | Char.AttrName | Char.AttrEnd);
  add(0x3a, 0x3a, Char.TagName | Char.AttrName | Char.AttrEnd); // :
  add(0x3b, 0x3b, Char.AttrEnd);
  add(0x3c, 0x3e, Char.AttrEnd); // < = >
  add(0x3f, 0x3f, Char.TagName | Char.AttrName | Char.AttrEnd); // ?
  add(0x41, 0x5a, Char.TagName | Char.TagNameStart | Char.AttrName | Char.HexDigit | Char.Alpha | Char.AttrEnd);
  add(0x5b, 0x5b, Char.AttrEnd);
  add(0x5d, 0x5d, Char.AttrEnd);
  add(0x5f, 0x5f, Char.TagName | Char.AttrName | Char.AttrEnd); // _
  add(0x61, 0x7a, Char.TagName | Char.TagNameStart | Char.AttrName | Char.HexDigit | Char.Alpha | Char.AttrEnd);

  return t;
}

const TABLE = build();

/**
 * The classification of `code`, or `Char.None` for anything outside ASCII.
 * Non-ASCII is never a tag or attribute character: a `<` followed by one is
 * prose (`<π`, `≤`) and must be left alone.
 */
export function classify(code: number): Char {
  return code <= ASCII_LAST ? TABLE[code] : Char.None;
}

/** Classify the character at `i`, without materialising a one-char string. */
export function classifyAt(s: string, i: number): Char {
  return classify(s.charCodeAt(i));
}

export function isSpaceAt(s: string, i: number): boolean {
  return (classifyAt(s, i) & Char.Space) !== 0;
}

export function isTagNameStartAt(s: string, i: number): boolean {
  return (classifyAt(s, i) & Char.TagNameStart) !== 0;
}

export function isTagNameCharAt(s: string, i: number): boolean {
  return (classifyAt(s, i) & Char.TagName) !== 0;
}

export function isAttrNameCharAt(s: string, i: number): boolean {
  return (classifyAt(s, i) & Char.AttrName) !== 0;
}

export function isAttrEndAt(s: string, i: number): boolean {
  return (classifyAt(s, i) & Char.AttrEnd) !== 0;
}

/** Index of the first character at or after `from` that is not a space. */
export function skipSpaces(s: string, from: number, limit: number): number {
  let i = from;
  while (i < limit && isSpaceAt(s, i)) i++;
  return i;
}

/** Index of the first newline in `s[from..limit)`, or `-1`. */
export function indexOfNewline(s: string, from: number, limit: number): number {
  for (let i = from; i < limit; i++) {
    if (s.charCodeAt(i) === 0x0a) return i;
  }
  return -1;
}

/**
 * A tag is never longer than this. A terminal command in an attribute can run
 * to a few hundred characters, but nothing legitimate runs to thousands, so a
 * `<` that has not closed within this budget is prose — a comparison written
 * with no `>`, or a stray character — and not a tag that is still arriving.
 *
 * Without this bound a single `<` with no partner would make the scanner hold
 * back the rest of the message waiting for a `>` that is never coming.
 */
export const MAX_TAG_LEN = 4096;
