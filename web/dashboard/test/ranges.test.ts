// Ranges, grains and comparison (ANA-71), matching
// `liyasa_analytics::query::Range`.

import test from "node:test";
import assert from "node:assert/strict";

import {
  bucketsOf,
  DAY_MS,
  describeRange,
  grainFor,
  HOUR_MS,
  lastDays,
  makeRange,
  previousRange,
  rangeSpan,
  resolveRange,
} from "../src/ranges.ts";

const T0 = 1_789_344_000_000; // 2026-09-14T00:00:00Z, a Monday

test("a range is half-open and reversed bounds are the same range", () => {
  assert.deepEqual(makeRange(T0 + DAY_MS, T0), { from: T0, to: T0 + DAY_MS });
  assert.equal(rangeSpan(makeRange(T0, T0 + 7 * DAY_MS)), 7 * DAY_MS);
});

test("the previous period abuts this one and is the same length", () => {
  const range = makeRange(T0, T0 + 7 * DAY_MS);
  const previous = previousRange(range);
  assert.equal(previous.to, range.from);
  assert.equal(rangeSpan(previous), rangeSpan(range));
});

test("last days means whole UTC days", () => {
  const now = T0 + 14 * HOUR_MS;
  const range = lastDays(now, 7);
  assert.equal(range.to, T0 + DAY_MS);
  assert.equal(range.from, T0 - 6 * DAY_MS);
  assert.equal(rangeSpan(range), 7 * DAY_MS);
  assert.equal(rangeSpan(lastDays(now, 0)), DAY_MS, "zero days is a day, not an empty window");
});

test("a relative spec follows the calendar and a pinned one does not", () => {
  const relative = resolveRange({ kind: "last", days: 28 }, T0);
  const later = resolveRange({ kind: "last", days: 28 }, T0 + 30 * DAY_MS);
  assert.notDeepEqual(relative, later);
  const pinned = { kind: "between", from: T0, to: T0 + DAY_MS } as const;
  assert.deepEqual(resolveRange(pinned, T0), resolveRange(pinned, T0 + 30 * DAY_MS));
});

test("the grain follows the span", () => {
  assert.equal(grainFor(makeRange(T0, T0 + DAY_MS)), "hour");
  assert.equal(grainFor(makeRange(T0, T0 + 2 * DAY_MS)), "hour");
  assert.equal(grainFor(makeRange(T0, T0 + 3 * DAY_MS)), "day");
});

test("buckets start on a boundary so a point's label is its own hour", () => {
  assert.deepEqual(bucketsOf(makeRange(T0, T0 + 3 * HOUR_MS), "hour"), [
    T0,
    T0 + HOUR_MS,
    T0 + 2 * HOUR_MS,
  ]);
  assert.deepEqual(bucketsOf(makeRange(T0 + 90 * 60_000, T0 + 3 * HOUR_MS), "hour"), [
    T0 + HOUR_MS,
    T0 + 2 * HOUR_MS,
  ]);
  assert.deepEqual(bucketsOf(makeRange(T0, T0 + DAY_MS), "day"), [T0]);
});

test("a range describes itself as a preset when it is one", () => {
  assert.equal(describeRange(lastDays(T0, 7)), "Last 7 days");
  assert.equal(describeRange(makeRange(T0, T0 + 3 * DAY_MS)), "2026-09-14 to 2026-09-16");
});
