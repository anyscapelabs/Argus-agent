const MS_PER_MIN = 60_000;
const MIN_PER_HR = 60;
const HR_PER_DAY = 24;
const WEEK_DAYS = 7;
const JUST_NOW = "just now";
const MIN_AGO = "m ago";
const HR_AGO = "h ago";
const YESTERDAY = "yesterday";
const DAYS_AGO = "days ago";

export function formatRelativeTime(
  input: Date | string | number,
): string {
  const date = input instanceof Date ? input : new Date(input);

  const now = Date.now();
  const diffMs = now - date.getTime();

  const diffMin = Math.floor(diffMs / MS_PER_MIN);
  if (diffMin < 1) return JUST_NOW;
  if (diffMin < MIN_PER_HR) return `${diffMin}${MIN_AGO}`;

  const diffHr = Math.floor(diffMin / MIN_PER_HR);
  if (diffHr < HR_PER_DAY) return `${diffHr}${HR_AGO}`;

  const diffDay = Math.floor(diffHr / HR_PER_DAY);
  if (diffDay === 1) return YESTERDAY;
  if (diffDay < WEEK_DAYS) return `${diffDay} ${DAYS_AGO}`;

  return date.toLocaleDateString(undefined, {
    month: "short",
    day: "numeric",
  });
}
