// Where a finished transcript lands in the composer.
//
// Dictation fills the box and stops. It never sends. A misheard word in a sent
// message is an executed instruction, and the whole reason this is a separate
// file is that the rule is worth a test of its own.

export type Placement = { text: string; caret: number };

export function insertTranscript(
  text: string,
  caret: number,
  transcript: string,
): Placement {
  const said = transcript.trim();
  if (said.length === 0) {
    return { text, caret };
  }

  // The box can change while a recording runs, so the caret saved at press
  // time is stale. Whatever the caller read at insert time gets clamped
  // rather than trusted.
  const at = Math.max(0, Math.min(caret, text.length));

  const before = text.slice(0, at);
  const after = text.slice(at);
  const spaced = before.length > 0 && !/\s$/.test(before) && !/^\s/.test(said)
    ? ` ${said}`
    : said;

  return {
    text: before + spaced + after,
    caret: at + spaced.length,
  };
}