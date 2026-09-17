// Numbers and dates for a screen. No Intl locale is chosen here: the dashboard
// renders in the operator's browser and `undefined` lets it pick, which is the
// one place a default is better than a decision.

/** `1,204`. */
export function formatCount(value: number): string {
  return new Intl.NumberFormat(undefined).format(Math.round(value));
}

/** `12%`, and `—` when there is nothing to take a percentage of. */
export function formatPercent(value: number | null, digits = 0): string {
  if (value === null || !Number.isFinite(value)) return "—";
  return new Intl.NumberFormat(undefined, {
    style: "percent",
    maximumFractionDigits: digits,
  }).format(value);
}

/**
 * `+12%`, `-3%`, `new`, or `—`.
 *
 * A change from nothing is `new` rather than `+100%`: the previous period was
 * zero and no percentage of zero is meaningful. Getting this wrong is how a
 * dashboard reports a page going from 0 to 1 view as its biggest riser.
 */
export function formatChange(current: number, previous: number): string {
  if (previous === 0) return current > 0 ? "new" : "—";
  const change = (current - previous) / previous;
  const sign = change >= 0 ? "+" : "";
  return sign + formatPercent(change);
}

/** `2026-09-14`, in UTC, matching what the API sends. */
export function formatDate(ms: number): string {
  return new Date(ms).toISOString().slice(0, 10);
}

/** `14 Sep 10:00`, for an hourly axis. */
export function formatHour(ms: number): string {
  return new Intl.DateTimeFormat(undefined, {
    day: "numeric",
    month: "short",
    hour: "2-digit",
    minute: "2-digit",
    timeZone: "UTC",
  }).format(new Date(ms));
}

/** `1.2 s`, `310 ms`. */
export function formatDuration(ms: number): string {
  if (ms >= 1000) return `${(ms / 1000).toFixed(1)} s`;
  return `${Math.round(ms)} ms`;
}

/** The label a bucket carries on an axis and in the data table. */
export function formatBucket(ms: number, grain: "hour" | "day"): string {
  return grain === "hour" ? formatHour(ms) : formatDate(ms);
}
