/// A whole line that is a command: the name, then anything after it.
export const SLASH_CALL = /^\/([a-z0-9]+)(?:\s+([\s\S]*))?$/i;

/// A leading `/` and nothing else yet, which is what opens the menu. Anything
/// with a space or a newline in it is a message, so `src/lib` and a pasted
/// path are never read as a command.
export const SLASH_MENU = /^\/([a-z0-9]*)$/i;

/// Whether the open command menu should swallow Enter, or Enter should send.
///
/// Scoped to a line that actually opened the menu. Every command name starts
/// with the empty string, so matching an unscoped empty query against the
/// registry matches the first command for ordinary text too — and Enter then
/// does nothing at all on every message.
export function menuTakesEnter(
  value: string,
  cmds: readonly { name: string }[],
): boolean {
  const m = SLASH_MENU.exec(value);

  if (m === null) {
    return false;
  }

  return cmds.some((c) => c.name.startsWith(m[1]));
}
