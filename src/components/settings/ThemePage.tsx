import { useEffect, useState } from "react";
import { LuLaptop, LuMoon, LuSun } from "react-icons/lu";

import {
  getTheme,
  setTheme,
  systemPrefersDark,
  THEME_MODES,
  type ThemeMode,
} from "../../lib/theme";

const ICONS: Record<ThemeMode, typeof LuSun> = {
  light: LuSun,
  dark: LuMoon,
  system: LuLaptop,
};

const BLURBS: Record<ThemeMode, string> = {
  light: "White background, dark text.",
  dark: "Near-black background, light text.",
  system: "Follows your desktop's appearance setting.",
};

export default function ThemePage() {
  const [mode, setMode] = useState<ThemeMode>(getTheme);
  const [sysDark, setSysDark] = useState(systemPrefersDark);

  useEffect(() => {
    const mq = window.matchMedia("(prefers-color-scheme: dark)");
    const onChange = () => setSysDark(mq.matches);

    mq.addEventListener("change", onChange);

    return () => mq.removeEventListener("change", onChange);
  }, []);

  function pick(next: ThemeMode) {
    setMode(next);
    setTheme(next);
  }

  const live: ThemeMode =
    mode === "system" ? (sysDark ? "dark" : "light") : mode;

  return (
    <div className="flex flex-col gap-2 px-2 py-3">
      <div>
        <h2 className="text-sm font-medium text-text-primary">Appearance</h2>
        <p className="text-xs text-text-secondary">
          Argus repaints immediately. Nothing reloads and no work is lost.
        </p>
      </div>

      <div className="mt-1 flex flex-col gap-1.5">
        {THEME_MODES.map((opt) => {
          const Icon = ICONS[opt.value];
          const active = opt.value === mode;

          return (
            <button
              key={opt.value}
              type="button"
              onClick={() => pick(opt.value)}
              aria-pressed={active}
              className={
                "flex items-center gap-3 rounded-lg border px-3 py-2.5 " +
                "text-left transition-colors " +
                (active
                  ? "border-accent bg-bg-hover-secondary"
                  : "border-border-primary hover:bg-bg-hover-primary")
              }
            >
              <Icon
                size={16}
                className={active ? "text-text-primary" : "text-text-secondary"}
              />
              <div className="min-w-0 flex-1">
                <div className="text-sm text-text-primary">{opt.label}</div>
                <div className="text-xs text-text-secondary">
                  {BLURBS[opt.value]}
                </div>
              </div>
              {opt.value === "system" && (
                <span className="shrink-0 text-xs text-text-secondary">
                  {sysDark ? "dark" : "light"}
                </span>
              )}
            </button>
          );
        })}
      </div>

      <p className="mt-1 text-xs text-text-secondary/70">
        Showing the {live} palette.
      </p>
    </div>
  );
}
