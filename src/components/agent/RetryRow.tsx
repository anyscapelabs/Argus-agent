import type { TurnRetry } from "../../stores/sessions";

export default function RetryRow({ retry }: { retry: TurnRetry }) {
  return (
    <div className="my-2 flex items-center gap-2.5 font-sans">
      <span className="flex h-5 w-5 shrink-0 items-center justify-center rounded-md border border-border-primary bg-bg-secondary">
        <span className="shimmer-text text-xs">◌</span>
      </span>
      <span className="shimmer-text text-sm">
        Rate-limited — retrying ({retry.attempt}/{retry.maxAttempts})
      </span>
      <span className="shrink-0 font-mono text-xs text-text-tertiary">
        next in {retry.waitSecs}s · {retry.label}
      </span>
    </div>
  );
}
