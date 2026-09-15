import { test } from "node:test";
import assert from "node:assert/strict";

import { BUDGET, collect, over } from "../src/vitals.ts";
import { fakeWindow } from "./dom.ts";
import type { FakeObserverConstructor, FakeWindow } from "./dom.ts";

function observers(win: FakeWindow) {
  const made = (win.PerformanceObserver as FakeObserverConstructor).created;
  return {
    of(type: string) {
      const found = made.find((observer) => observer.type === type);
      assert.ok(found, `nothing observes \`${type}\``);
      return found;
    },
  };
}

test("the budget is the one RX-11 sets", () => {
  assert.deepEqual(BUDGET, { lcp: 1200, cls: 0.05, inp: 100 });
});

test("the largest contentful paint is the last one reported", () => {
  const win = fakeWindow();
  const vitals = collect(win as never);
  const lcp = observers(win).of("largest-contentful-paint");
  lcp.emit([{ startTime: 400 }]);
  lcp.emit([{ startTime: 980 }]);
  assert.equal(vitals.snapshot().lcp, 980);
});

test("layout shift is summed per session window, and the worst window wins", () => {
  const win = fakeWindow();
  const vitals = collect(win as never);
  const shifts = observers(win).of("layout-shift");
  // One window: three shifts inside a second of each other.
  shifts.emit([
    { value: 0.01, startTime: 100, hadRecentInput: false },
    { value: 0.02, startTime: 400, hadRecentInput: false },
    { value: 0.01, startTime: 900, hadRecentInput: false },
  ]);
  assert.equal(Number(vitals.snapshot().cls.toFixed(4)), 0.04);
  // A gap over a second starts a new window rather than adding to the old one.
  shifts.emit([{ value: 0.03, startTime: 4000, hadRecentInput: false }]);
  assert.equal(Number(vitals.snapshot().cls.toFixed(4)), 0.04);
  shifts.emit([{ value: 0.02, startTime: 4500, hadRecentInput: false }]);
  assert.equal(Number(vitals.snapshot().cls.toFixed(4)), 0.05);
});

test("a shift the reader caused is not counted", () => {
  const win = fakeWindow();
  const vitals = collect(win as never);
  observers(win)
    .of("layout-shift")
    .emit([{ value: 0.5, startTime: 100, hadRecentInput: true }]);
  assert.equal(vitals.snapshot().cls, 0);
});

test("interaction latency is the slowest interaction", () => {
  const win = fakeWindow();
  const vitals = collect(win as never);
  const events = observers(win).of("event");
  events.emit([
    { duration: 40, interactionId: 1 },
    { duration: 120, interactionId: 2 },
    { duration: 900, interactionId: 0 },
  ]);
  assert.equal(vitals.snapshot().inp, 120);
});

test("nothing is reported before the browser reports it", () => {
  const win = fakeWindow();
  assert.deepEqual(collect(win as never).snapshot(), { lcp: null, cls: 0, inp: null });
});

test("an entry type the browser does not support is skipped, not thrown", () => {
  const win = fakeWindow({ entryTypes: ["layout-shift"] });
  const vitals = collect(win as never);
  assert.equal(vitals.snapshot().lcp, null);
  observers(win)
    .of("layout-shift")
    .emit([{ value: 0.02, startTime: 10, hadRecentInput: false }]);
  assert.equal(Number(vitals.snapshot().cls.toFixed(4)), 0.02);
});

test("a measurement over budget names the metric", () => {
  assert.deepEqual(over({ lcp: 1199, cls: 0.049, inp: 99 }), []);
  assert.deepEqual(over({ lcp: null, cls: 0, inp: null }), []);
  assert.deepEqual(over({ lcp: 1400, cls: 0.2, inp: 260 }), [
    "LCP 1400 ms, over 1200 ms",
    "CLS 0.2, over 0.05",
    "INP 260 ms, over 100 ms",
  ]);
});
