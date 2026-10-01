export const SLASH_CALL = /^\/([a-z0-9]+)(?:\s+([\s\S]*))?$/i;

/// A leading `/` and nothing else, which is what opens the menu. A space or a
/// newline makes it a message, so `src/lib` and a pasted path are never read as
/// a command.
export const SLASH_MENU = /^\/([a-z0-9]*)$/i;

/// Scoped to a line that actually opened the menu. Every command name starts
/// with the empty string, so an unscoped empty query matches the first command
/// for ordinary text too — and Enter then does nothing on every message.
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
