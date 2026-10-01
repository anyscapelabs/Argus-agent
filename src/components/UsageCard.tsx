import { useEffect, useState } from "react";

import { slashRun, type Usage, type UsageWindow } from "../lib/ipc";

const WINDOWS: { key: UsageWindow; label: string }[] = [
  { key: "today", label: "Today" },
  { key: "week", label: "Week" },
  { key: "month", label: "Month" },
];

type Props = {
  /// The stored turn carries the window, so `/usage month` comes back on Month.
  window?: UsageWindow;
};

export default function UsageCard({ window: start = "week" }: Props) {
  const [win, setWin] = useState<UsageWindow>(start);
  const [report, setReport] = useState<Usage | null>(null);

  // A view, not a snapshot: the turn stores what the user typed and the
  // numbers are read when the card is drawn.
  useEffect(() => {
    let live = true;
    setReport(null);

    void slashRun("usage", null, win)
      .then((out) => {
        if (live && out.usage !== null) {
          setReport(out.usage);
        }
      })
      // Not worth blanking the card, and nothing to retry: the next tab change
      // asks again.
      .catch(() => {});

    return () => {
      live = false;
    };
  }, [win]);

  const rate =
    report !== null && report.requests > 0 ? report.failed / report.requests : 0;
  const empty =
    report !== null && report.requests === 0 && report.days.length === 0;

  return (
    <div
      className={
        "w-full max-w-[700px] rounded-2xl border border-border-primary " +
        "bg-bg-secondary px-5 py-4"
      }
    >
      <div className="mb-4 flex items-center justify-between gap-3">
        <div className="text-sm font-medium text-text-primary">Usage</div>

        <div className="flex shrink-0 gap-1">
          {WINDOWS.map((w) => {
            const on = w.key === win;

            return (
              <button
                key={w.key}
                type="button"
                onClick={() => setWin(w.key)}
                className={
                  "rounded-md px-2.5 py-1 text-xs transition-colors " +
                  (on
                    ? "bg-bg-hover-secondary text-text-primary"
                    : "text-text-tertiary hover:text-text-secondary")
                }
              >
                {w.label}
              </button>
            );
          })}
        </div>
      </div>

      {report === null ? (
        <Loading />
      ) : (
        <>
          <div className="grid grid-cols-5 divide-x divide-border-primary">
            <Stat n={report.requests.toLocaleString()} label="Requests" />
            <Stat n={tokens(report.tokIn + report.tokOut)} label="Tokens" />
            <Stat n={money(report.cost)} label="Spend" />
            <Stat n={duration(report.workedMs)} label="Worked for" />
            <Stat
              n={`${(rate * 100).toFixed(rate > 0 && rate < 0.01 ? 1 : 0)}%`}
              label={report.failed > 0 ? `Failed · ${report.failed}` : "Failed"}
            />
          </div>

          <div className="mt-5 mb-2 flex items-baseline justify-between">
            <div className="text-sm text-text-primary">Activity</div>
            <div className="text-xs text-text-tertiary">{report.label}</div>
          </div>

          <Calendar days={report.days} window={report.window} />

          {empty ? (
            <div className="mt-4 text-sm text-text-tertiary">
              Nothing sent in this window yet.
            </div>
          ) : (
            <Models models={report.models} />
          )}
        </>
      )}
    </div>
  );
}

// The same height the numbers take, so the card does not jump when they land.
function Loading() {
  return (
    <div className="h-[168px] animate-pulse rounded-xl bg-bg-hover-primary" />
  );
}

function Stat({ n, label }: { n: string; label: string }) {
  return (
    <div className="min-w-0 px-3 py-1 first:pl-0 last:pr-0">
      <div className="truncate font-mono text-base text-text-primary">{n}</div>
      <div className="mt-0.5 truncate text-xs text-text-tertiary">{label}</div>
    </div>
  );
}

function Calendar({
  days,
  window,
}: {
  days: Usage["days"];
  window: UsageWindow;
}) {
  if (days.length === 0) {
    return <div className="h-[52px]" />;
  }

  // Scaled to this window's busiest day: a fixed threshold makes every month
  // look like the same quiet month.
  const peak = Math.max(...days.map((d) => d.tokens), 1);
  const cols = { gridTemplateColumns: `repeat(${days.length}, minmax(0, 1fr))` };

  return (
    <div className="overflow-x-auto">
      <div className="grid gap-1" style={cols}>
        {days.map((d) => (
          <div
            key={d.day}
            title={`${d.day} · ${tokens(d.tokens)} tokens · ${money(d.cost)}`}
            className={
              "mx-auto h-3 w-full max-w-[36px] rounded-[3px] " +
              level(d.tokens / peak)
            }
          />
        ))}
      </div>

      <div className="mt-1.5 grid gap-1" style={cols}>
        {days.map((d) => (
          <span
            key={d.day}
            className="truncate text-center text-[10px] tabular-nums text-text-tertiary"
          >
            {tick(d.day, window)}
          </span>
        ))}
      </div>
    </div>
  );
}

// A week reads as weekdays, a month as days of the month.
const WEEKDAY = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];

function tick(day: string, window: UsageWindow): string {
  if (window === "week") {
    const at = Date.parse(`${day}T00:00:00`);
    return Number.isNaN(at) ? day.slice(5) : WEEKDAY[new Date(at).getDay()];
  }

  return day.slice(8);
}

// Decades, not quarters: one heavy day is often twenty times a quiet one, and
// quarter cuts paint all six quiet days the same grey.
function level(ratio: number): string {
  if (ratio <= 0) {
    return "bg-bg-hover-primary";
  }

  if (ratio > 0.5) return "bg-accent";
  if (ratio > 0.1) return "bg-accent/70";
  if (ratio > 0.03) return "bg-accent/40";
  return "bg-accent/20";
}

function Models({ models }: { models: Usage["models"] }) {
  if (models.length === 0) {
    return null;
  }

  return (
    <div className="mt-5 border-t border-border-primary pt-3">
      {models.map((m) => (
        <div
          key={`${m.model}-${m.provider}`}
          className="flex items-baseline justify-between gap-4 py-1 text-sm"
        >
          <div className="min-w-0 truncate text-text-primary">
            {m.model}
            {m.provider !== "" && (
              <span className="ml-2 text-xs text-text-tertiary">{m.provider}</span>
            )}
          </div>

          <div className="shrink-0 text-xs text-text-tertiary">
            {m.requests} req · {tokens(m.tokens)} · {money(m.cost)}
          </div>
        </div>
      ))}
    </div>
  );
}

function tokens(n: number): string {
  if (n >= 1_000_000) {
    return `${(n / 1_000_000).toFixed(1)}M`;
  }

  if (n >= 1_000) {
    return `${Math.round(n / 1_000)}k`;
  }

  return n.toString();
}

// A cent under is not $0.00, and the point of the number is that it is not
// zero.
function money(cost: number): string {
  if (cost > 0 && cost < 0.01) {
    return "under a cent";
  }

  return `$${cost.toFixed(2)}`;
}

// Same shape as the transcript's "Worked for" label, so both read as one unit.
// Seconds drop out once they are the smaller half, which is also when they
// would wrap the strip.
function duration(ms: number): string {
  const sec = Math.max(0, Math.round(ms / 1000));

  if (sec < 60) {
    return `${sec} sec`;
  }

  const mins = Math.floor(sec / 60);
  const rem = sec % 60;

  if (mins < 60) {
    if (rem === 0 || mins >= 10) {
      return `${mins} min`;
    }

    return `${mins} min ${rem} sec`;
  }

  const hours = Math.floor(mins / 60);
  const remH = mins % 60;

  return remH === 0 ? `${hours} hr` : `${hours} hr ${remH} min`;
}
