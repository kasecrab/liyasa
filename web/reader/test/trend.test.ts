import { test } from "node:test";
import assert from "node:assert/strict";

import { GATE, below, line, parse, slipped } from "../e2e/trend.ts";
import type { Scores } from "../e2e/trend.ts";

const PERFECT: Scores = { performance: 100, accessibility: 100, "best-practices": 100, seo: 100 };

function at(performance: number): Scores {
  return { ...PERFECT, performance };
}

test("the gate is the one RX-10 sets", () => {
  assert.deepEqual(GATE, {
    performance: 98,
    accessibility: 100,
    "best-practices": 100,
    seo: 100,
  });
});

test("a perfect run passes the gate", () => {
  assert.deepEqual(below(PERFECT), []);
});

test("performance may lose two points and still release", () => {
  assert.deepEqual(below(at(98)), []);
  assert.deepEqual(below(at(97)), ["performance 97, under 98"]);
});

test("the other three categories are exact", () => {
  assert.deepEqual(below({ ...PERFECT, seo: 99 }), ["seo 99, under 100"]);
  assert.deepEqual(below({ ...PERFECT, accessibility: 99, "best-practices": 99 }), [
    "accessibility 99, under 100",
    "best-practices 99, under 100",
  ]);
});

test("a run is one json line naming the route and the commit", () => {
  const text = line("/guide/install", PERFECT, "abc1234", "2026-09-15T00:00:00.000Z");
  assert.equal(text.endsWith("\n"), true);
  const record = JSON.parse(text);
  assert.equal(record.route, "/guide/install");
  assert.equal(record.commit, "abc1234");
  assert.equal(record.at, "2026-09-15T00:00:00.000Z");
  assert.equal(record.performance, 100);
});

test("a trend file is read back as the runs it recorded", () => {
  const text = line("/", PERFECT, "a", "2026-09-15T00:00:00.000Z") + line("/", at(99), "b", "2026-09-15T01:00:00.000Z");
  const runs = parse(text);
  assert.equal(runs.length, 2);
  assert.equal(runs[1]?.performance, 99);
});

test("a blank or broken line does not lose the trend", () => {
  const text = `${line("/", PERFECT, "a", "2026-09-15T00:00:00.000Z")}\nnot json\n`;
  assert.equal(parse(text).length, 1);
});

test("a one-point drop is reported and is not a failure", () => {
  const history = parse(line("/", PERFECT, "a", "2026-09-15T00:00:00.000Z"));
  const dropped = at(99);
  assert.deepEqual(slipped(history, "/", dropped), ["performance 100 to 99"]);
  // The release is not blocked: the score is still over the gate.
  assert.deepEqual(below(dropped), []);
});

test("a route is compared only against itself", () => {
  const history = parse(line("/reference/cli", at(92), "a", "2026-09-15T00:00:00.000Z"));
  assert.deepEqual(slipped(history, "/", at(99)), []);
});

test("the newest run for the route is the one compared against", () => {
  const history = parse(
    line("/", at(94), "a", "2026-09-15T00:00:00.000Z") + line("/", at(99), "b", "2026-09-15T01:00:00.000Z"),
  );
  assert.deepEqual(slipped(history, "/", at(99)), []);
  assert.deepEqual(slipped(history, "/", at(98)), ["performance 99 to 98"]);
});

test("a score that improved is not a slip", () => {
  const history = parse(line("/", at(98), "a", "2026-09-15T00:00:00.000Z"));
  assert.deepEqual(slipped(history, "/", PERFECT), []);
});

test("the first run of a route has nothing to slip from", () => {
  assert.deepEqual(slipped([], "/", at(98)), []);
});
