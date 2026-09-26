import { useEffect, useState } from "react";

import {
  getTheme,
  setTheme,
  systemPrefersDark,
  THEME_MODES,
  type ThemeMode,
} from "../../lib/theme";
import { Page, Section } from "./kit";

/// A miniature of the window, drawn in the palette it stands for. Three grey
/// bars and a sidebar beat a sun-and-moon icon every time: you can see what you
/// are picking, not just what it is called.
function Preview({ mode, sysDark }: { mode: ThemeMode; sysDark: boolean }) {
  const dark = mode === "system" ? sysDark : mode === "dark";
  const surface = dark ? "#2a2a2a" : "#f4f4f2";
  const chrome = dark ? "#1f1f1f" : "#e8e8e4";
  const bar = dark ? "#4a4a4a" : "#c8c8c2";
  const side = dark ? "#383838" : "#dcdcd6";

  return (
    <div
      className="flex aspect-4/3 w-full overflow-hidden rounded-lg"
      style={{ background: chrome }}
    >
      <div className="w-1/4 p-2" style={{ background: side }}>
        <div className="mb-1.5 h-1 w-3/4 rounded-full" style={{ background: bar }} />
        <div className="mb-1.5 h-1 w-3/4 rounded-full" style={{ background: bar }} />
        <div className="h-1 w-1/2 rounded-full" style={{ background: bar }} />
      </div>
      <div className="flex flex-1 flex-col gap-1.5 p-2.5">
        <div
          className="h-1.5 w-1/2 rounded-full"
          style={{ background: bar }}
        />
        <div
          className="flex-1 rounded-md p-2"
          style={{ background: surface }}
        >
          <div
            className="mb-1.5 h-1 w-4/5 rounded-full"
            style={{ background: bar }}
          />
          <div
            className="mb-1.5 h-1 w-3/5 rounded-full"
            style={{ background: bar }}
          />
          <div className="h-1 w-2/5 rounded-full" style={{ background: bar }} />
        </div>
      </div>
    </div>
  );
}

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

  return (
    <Page>
      <Section label="Theme">
        <div className="grid grid-cols-3 gap-4">
          {THEME_MODES.map((opt) => {
            const active = opt.value === mode;

            return (
              <button
                key={opt.value}
                type="button"
                onClick={() => pick(opt.value)}
                aria-pressed={active}
                className="flex flex-col gap-2.5 focus:outline-none"
              >
                <div
                  className={
                    "overflow-hidden rounded-xl border-2 p-1.5 transition-colors " +
                    (active
                      ? "border-text-primary"
                      : "border-transparent hover:border-border-primary")
                  }
                >
                  <Preview mode={opt.value} sysDark={sysDark} />
                </div>
                <span
                  className={
                    "text-xs " +
                    (active
                      ? "text-text-primary"
                      : "text-text-secondary")
                  }
                >
                  {opt.label}
                </span>
              </button>
            );
          })}
        </div>
      </Section>

    </Page>
  );
}
