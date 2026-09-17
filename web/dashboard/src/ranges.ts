// Date ranges, grains and period-over-period comparison (ANA-71).
//
// A range is half-open: `[from, to)`. The end of one period is the start of
// the next, so an event is counted once whichever of the two you ask about.

export const HOUR_MS = 3_600_000;
export const DAY_MS = 86_400_000;

export type Grain = "hour" | "day";

export interface Range {
  from: number;
  to: number;
}

export type RangeSpec = { kind: "last"; days: number } | { kind: "between"; from: number; to: number };

/** The presets the date picker offers. */
export const RANGE_PRESETS: Array<{ id: string; label: string; days: number }> = [
  { id: "today", label: "Today", days: 1 },
  { id: "7d", label: "Last 7 days", days: 7 },
  { id: "28d", label: "Last 28 days", days: 28 },
  { id: "90d", label: "Last 90 days", days: 90 },
  { id: "12m", label: "Last 12 months", days: 365 },
];

export function makeRange(from: number, to: number): Range {
  return { from: Math.min(from, to), to: Math.max(from, to) };
}

/**
 * The last `days` whole UTC days ending at the midnight after `now`.
 *
 * Whole days rather than a rolling window from the current instant: "last 7
 * days" compared against "the 7 days before that" is only a comparison if both
 * cover the same hours of the week.
 */
export function lastDays(now: number, days: number): Range {
  const end = now - modulo(now, DAY_MS) + DAY_MS;
  return makeRange(end - Math.max(1, days) * DAY_MS, end);
}

export function resolveRange(spec: RangeSpec, now: number): Range {
  return spec.kind === "last" ? lastDays(now, spec.days) : makeRange(spec.from, spec.to);
}

export function rangeSpan(range: Range): number {
  return range.to - range.from;
}

/** The window immediately before this one, for period over period. */
export function previousRange(range: Range): Range {
  const span = rangeSpan(range);
  return makeRange(range.from - span, range.from);
}

export function grainMillis(grain: Grain): number {
  return grain === "hour" ? HOUR_MS : DAY_MS;
}

/** Hours up to two days, days beyond: 8,760 points is not a chart. */
export function grainFor(range: Range): Grain {
  return rangeSpan(range) <= 2 * DAY_MS ? "hour" : "day";
}

/** Every bucket boundary the range covers, so a quiet hour is a zero. */
export function bucketsOf(range: Range, grain: Grain): number[] {
  const step = grainMillis(grain);
  const out: number[] = [];
  for (let at = range.from - modulo(range.from, step); at < range.to; at += step) out.push(at);
  return out;
}

export function describeRange(range: Range): string {
  const days = Math.round(rangeSpan(range) / DAY_MS);
  const preset = RANGE_PRESETS.find((p) => p.days === days);
  return preset ? preset.label : `${new Date(range.from).toISOString().slice(0, 10)} to ${new Date(range.to - 1).toISOString().slice(0, 10)}`;
}

/** `%` is remainder in JavaScript, not modulo; a negative instant needs this. */
function modulo(value: number, by: number): number {
  return ((value % by) + by) % by;
}
