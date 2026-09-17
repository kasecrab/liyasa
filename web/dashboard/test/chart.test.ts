// ANA-70: "charts are accessible (keyboard, data tables toggle)".
//
// The table is asserted against the same series the line is drawn from, so the
// two cannot drift; the readout is asserted against the same numbers again.

import test from "node:test";
import assert from "node:assert/strict";

import {
  chartMax,
  chartPath,
  chartReadout,
  chartSummary,
  chartTable,
  chartTotals,
  renderChart,
} from "../src/chart.ts";
import type { ChartSpec } from "../src/chart.ts";

const DAY = 86_400_000;
const T0 = 1_789_344_000_000; // 2026-09-14T00:00:00Z

function spec(): ChartSpec {
  return {
    id: "traffic",
    title: "Page views",
    grain: "day",
    kind: "line",
    buckets: [T0, T0 + DAY, T0 + 2 * DAY],
    series: [
      { key: "human", label: "Readers", values: [10, 20, 5] },
      { key: "agent", label: "Agents", values: [1, 0, 7] },
    ],
  };
}

test("every chart carries a table of the numbers it was drawn from", () => {
  const table = String(chartTable(spec()));
  // One header per series plus the bucket column.
  assert.match(table, /<th scope="col">When<\/th>/);
  assert.match(table, /<th scope="col">Readers<\/th>/);
  assert.match(table, /<th scope="col">Agents<\/th>/);
  // One row per bucket, with both series' values.
  const rows = table.match(/<tr>\s*<th scope="row">/g) ?? [];
  assert.equal(rows.length, 3);
  assert.match(table, /2026-09-15[\s\S]*?<td>20<\/td>[\s\S]*?<td>0<\/td>/);
  // And it is a toggle, which is what ANA-70 asks for.
  assert.match(table, /<details/);
  assert.match(table, /<summary>Show the numbers<\/summary>/);
});

test("the table is in the markup whether or not anyone opens it", () => {
  const markup = String(renderChart(spec()));
  assert.match(markup, /<details class="ly-chart-data">/);
  assert.ok(
    markup.indexOf("<table") > markup.indexOf("<svg"),
    "a screen reader reaches the numbers without toggling anything",
  );
});

test("the plot takes focus once rather than putting every point in the tab order", () => {
  const many: ChartSpec = {
    ...spec(),
    buckets: Array.from({ length: 365 }, (_, i) => T0 + i * DAY),
    series: [{ key: "human", label: "Readers", values: Array.from({ length: 365 }, () => 1) }],
  };
  const markup = String(renderChart(many));
  const focusable = markup.match(/tabindex="0"/g) ?? [];
  assert.equal(focusable.length, 1, "365 tab stops is a wall, not accessibility");
  assert.match(markup, /role="img"/);
  assert.match(markup, /aria-label="Page views, 2026-09-14 to 2027-09-13/);
  assert.match(markup, /aria-live="polite"/);
});

test("the readout says what the cursor is on, in the table's numbers", () => {
  assert.equal(chartReadout(spec(), 0), "2026-09-14: Readers 10, Agents 1");
  assert.equal(chartReadout(spec(), 2), "2026-09-16: Readers 5, Agents 7");
  assert.equal(chartReadout(spec(), 9), "", "past the end says nothing rather than undefined");
});

test("the accessible name summarises rather than repeating the table", () => {
  const summary = chartSummary(spec());
  assert.match(summary, /^Page views, 2026-09-14 to 2026-09-16\./);
  assert.match(summary, /Readers: 35 total/);
  assert.match(summary, /Agents: 8 total/);
});

test("totals and the maximum come from the series", () => {
  assert.deepEqual(chartTotals(spec()), { human: 35, agent: 8 });
  assert.equal(chartMax(spec()), 20);
});

test("a chart of nothing still has an axis and does not divide by zero", () => {
  const flat: ChartSpec = {
    ...spec(),
    series: [{ key: "human", label: "Readers", values: [0, 0, 0] }],
  };
  assert.equal(chartMax(flat), 1);
  const path = chartPath(flat, flat.series[0]!);
  assert.ok(!path.includes("NaN"), path);
  const markup = String(renderChart(flat));
  assert.ok(!markup.includes("NaN"), "a flat zero draws a line on the baseline");
});

test("a single point is a chart and not a division by zero", () => {
  const one: ChartSpec = {
    ...spec(),
    buckets: [T0],
    series: [{ key: "human", label: "Readers", values: [4] }],
  };
  assert.ok(!chartPath(one, one.series[0]!).includes("NaN"));
  assert.equal(chartReadout(one, 0), "2026-09-14: Readers 4");
});

test("a series label that looks like markup does not become markup", () => {
  const hostile: ChartSpec = {
    ...spec(),
    title: '</svg><script>alert(1)</script>',
    series: [{ key: "human", label: '"><img onerror=alert(1)>', values: [1, 2, 3] }],
  };
  const markup = String(renderChart(hostile));
  assert.ok(!markup.includes("<script>"), markup.slice(0, 200));
  assert.ok(!markup.includes("<img"), markup.slice(0, 200));
  assert.match(markup, /&lt;script&gt;/);
});

test("a client measured chart carries ANA-10's label", () => {
  const sampled: ChartSpec = { ...spec(), note: "Sampled by client — 71% delivered" };
  assert.match(String(renderChart(sampled)), /Sampled by client — 71% delivered/);
  assert.ok(!String(renderChart(spec())).includes("ly-chart-note"));
});
