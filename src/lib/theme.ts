export type ThemeMode = "light" | "dark" | "system";

const KEY = "argus.theme";
const QUERY = "(prefers-color-scheme: dark)";

export const THEME_MODES: { value: ThemeMode; label: string }[] = [
  { value: "light", label: "Light" },
  { value: "dark", label: "Dark" },
  { value: "system", label: "System" },
];

function isMode(v: string | null): v is ThemeMode {
  return v === "light" || v === "dark" || v === "system";
}

export function getTheme(): ThemeMode {
  try {
    const v = window.localStorage.getItem(KEY);
    return isMode(v) ? v : "system";
  } catch {
    return "system";
  }
}

export function systemPrefersDark(): boolean {
  return window.matchMedia(QUERY).matches;
}

function paint(mode: ThemeMode) {
  const root = document.documentElement;

  if (mode === "system") {
    root.removeAttribute("data-theme");
    return;
  }

  root.setAttribute("data-theme", mode);
}

export function setTheme(mode: ThemeMode) {
  if (!isMode(mode)) {
    paint("system");
    return;
  }

  try {
    window.localStorage.setItem(KEY, mode);
  } catch {
    // Private mode or a locked webview: the attribute still applies.
  }

  paint(mode);
}

/**
 * Follows the OS while the mode is `system`, so a desktop theme change
 * repaints without a reload. No-op once the user picks light or dark.
 */
export function watchSystemTheme(): () => void {
  const mq = window.matchMedia(QUERY);
  const onChange = () => {
    if (getTheme() === "system") paint("system");
  };

  mq.addEventListener("change", onChange);

  return () => mq.removeEventListener("change", onChange);
}
