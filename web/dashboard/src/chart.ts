// Charts (ANA-70: "charts are accessible (keyboard, data tables toggle) and
// follow the design system in light and dark").
//
// Three decisions hold that up.
//
// The data table is not a fallback. It is in the DOM for every chart, always,
// inside a `<details>` whose `<summary>` is the toggle — so a screen reader
// reaches the numbers without anyone toggling anything, and a sighted reader
// who wants the numbers gets the same ones the line was drawn from rather than
// a second query that might disagree.
//
// Keyboard navigation moves a cursor rather than tabbing through points. A
// year of daily traffic is 365 points; putting each one in the tab order makes
// the chart a wall to everyone who navigates that way. The figure takes focus
// once, arrow keys move the cursor, and an `aria-live` readout says what is
// under it.
//
// Every colour and length is a `--ly-*` token from the theme, so light and
// dark follow the design system rather than being redefined here.

import { escapeHtml, html, raw } from "./escape.ts";
import type { Fragment } from "./escape.ts";
import { formatBucket, formatCount } from "./format.ts";
import type { Grain } from "./ranges.ts";

export interface ChartSeries {
  key: string;
  label: string;
  values: number[];
}

export interface ChartSpec {
  id: string;
  title: string;
  grain: Grain;
  /** Bucket boundaries; one per value in every series. */
  buckets: number[];
  series: ChartSeries[];
  kind: "line" | "bar";
  /**
   * ANA-10: a client-measured series is an undercount and says so, with the
   * measured beacon delivery ratio beside it.
   */
  note?: string;
}

const WIDTH = 720;
const HEIGHT = 180;
const PAD_X = 4;
const PAD_Y = 6;

/** The largest value any series reaches; at least 1, so a flat zero has an axis. */
export function chartMax(spec: ChartSpec): number {
  let max = 0;
  for (const series of spec.series) {
    for (const value of series.values) if (value > max) max = value;
  }
  return Math.max(1, max);
}

/** Totals per series, which the legend shows. */
export function chartTotals(spec: ChartSpec): Record<string, number> {
  const totals: Record<string, number> = {};
  for (const series of spec.series) {
    totals[series.key] = series.values.reduce((sum, value) => sum + value, 0);
  }
  return totals;
}

function pointX(index: number, count: number): number {
  if (count <= 1) return PAD_X;
  return PAD_X + (index * (WIDTH - 2 * PAD_X)) / (count - 1);
}

function pointY(value: number, max: number): number {
  return HEIGHT - PAD_Y - (value / max) * (HEIGHT - 2 * PAD_Y);
}

/** The `d` of one series' line, rounded so the markup is stable between runs. */
export function chartPath(spec: ChartSpec, series: ChartSeries): string {
  const max = chartMax(spec);
  const count = spec.buckets.length;
  return series.values
    .map((value, index) => {
      const x = pointX(index, count).toFixed(1);
      const y = pointY(value, max).toFixed(1);
      return `${index === 0 ? "M" : "L"}${x} ${y}`;
    })
    .join(" ");
}

/**
 * What the live region says when the cursor is on `index`.
 *
 * Pure, and the same numbers the table row carries: the announcement and the
 * table cannot drift apart.
 */
export function chartReadout(spec: ChartSpec, index: number): string {
  const bucket = spec.buckets[index];
  if (bucket === undefined) return "";
  const parts = spec.series.map(
    (series) => `${series.label} ${formatCount(series.values[index] ?? 0)}`,
  );
  return `${formatBucket(bucket, spec.grain)}: ${parts.join(", ")}`;
}

/** The one-sentence summary the SVG carries as its accessible name. */
export function chartSummary(spec: ChartSpec): string {
  const totals = chartTotals(spec);
  const parts = spec.series.map((s) => `${series_total_phrase(s, totals[s.key] ?? 0)}`);
  const span =
    spec.buckets.length > 0
      ? `${formatBucket(spec.buckets[0] as number, spec.grain)} to ${formatBucket(
          spec.buckets[spec.buckets.length - 1] as number,
          spec.grain,
        )}`
      : "no data";
  return `${spec.title}, ${span}. ${parts.join(". ")}.`;
}

function series_total_phrase(series: ChartSeries, total: number): string {
  return `${series.label}: ${formatCount(total)} total`;
}

/** The table every chart carries, which is also the toggle of ANA-70. */
export function chartTable(spec: ChartSpec): Fragment {
  const rows = spec.buckets.map((bucket, index) =>
    html`<tr>
      <th scope="row">${formatBucket(bucket, spec.grain)}</th>
      ${spec.series.map((series) => html`<td>${formatCount(series.values[index] ?? 0)}</td>`)}
    </tr>`,
  );
  return html`<details class="ly-chart-data">
    <summary>Show the numbers</summary>
    <table>
      <caption>
        ${spec.title}
      </caption>
      <thead>
        <tr>
          <th scope="col">When</th>
          ${spec.series.map((series) => html`<th scope="col">${series.label}</th>`)}
        </tr>
      </thead>
      <tbody>
        ${rows}
      </tbody>
    </table>
  </details>`;
}

export function renderChart(spec: ChartSpec): Fragment {
  const max = chartMax(spec);
  const count = spec.buckets.length;
  const totals = chartTotals(spec);
  const marks = spec.series.map((series, order) => {
    if (spec.kind === "bar") {
      const width = count > 0 ? Math.max(1, (WIDTH - 2 * PAD_X) / count - 1) : 1;
      return series.values
        .map((value, index) => {
          const x = pointX(index, count);
          const y = pointY(value, max);
          return `<rect class="ly-chart-bar" data-series="${escapeHtml(series.key)}" x="${x.toFixed(1)}" y="${y.toFixed(1)}" width="${width.toFixed(1)}" height="${(HEIGHT - PAD_Y - y).toFixed(1)}" />`;
        })
        .join("");
    }
    return `<path class="ly-chart-line" data-series="${escapeHtml(series.key)}" data-order="${order}" d="${chartPath(spec, series)}" />`;
  });

  return html`<figure class="ly-chart" data-chart="${spec.id}">
    <figcaption>
      <span class="ly-chart-title" id="${spec.id}-title">${spec.title}</span>
      ${spec.note ? html`<span class="ly-chart-note">${spec.note}</span>` : ""}
    </figcaption>
    <ul class="ly-chart-legend">
      ${spec.series.map(
        (series) => html`<li data-series="${series.key}">
          <span class="ly-chart-swatch" aria-hidden="true"></span>${series.label}
          <b>${formatCount(totals[series.key] ?? 0)}</b>
        </li>`,
      )}
    </ul>
    <svg
      class="ly-chart-plot"
      viewBox="0 0 ${String(WIDTH)} ${String(HEIGHT)}"
      preserveAspectRatio="none"
      role="img"
      tabindex="0"
      aria-describedby="${spec.id}-readout"
      aria-label="${chartSummary(spec)}"
    >
      ${raw(marks.join(""))}
      <line class="ly-chart-cursor" x1="0" y1="0" x2="0" y2="${String(HEIGHT)}" hidden />
    </svg>
    <p class="ly-chart-readout" id="${spec.id}-readout" aria-live="polite"></p>
    ${chartTable(spec)}
  </figure>`;
}

/**
 * Arrow keys move the cursor; Home and End jump to the ends.
 *
 * Called on a mounted subtree. Everything it needs is in the markup, so a
 * re-render re-attaches without any state to carry over.
 */
export function attachChartKeys(root: ParentNode, specs: Map<string, ChartSpec>): void {
  for (const figure of Array.from(root.querySelectorAll("[data-chart]"))) {
    const id = figure.getAttribute("data-chart") ?? "";
    const spec = specs.get(id);
    const plot = figure.querySelector(".ly-chart-plot");
    const readout = figure.querySelector(".ly-chart-readout");
    const cursor = figure.querySelector(".ly-chart-cursor");
    if (!spec || !plot || !readout) continue;
    let index = -1;
    plot.addEventListener("keydown", (event) => {
      const key = (event as KeyboardEvent).key;
      const last = spec.buckets.length - 1;
      if (last < 0) return;
      let next = index;
      if (key === "ArrowRight") next = index < 0 ? 0 : Math.min(last, index + 1);
      else if (key === "ArrowLeft") next = index < 0 ? last : Math.max(0, index - 1);
      else if (key === "Home") next = 0;
      else if (key === "End") next = last;
      else return;
      event.preventDefault();
      index = next;
      readout.textContent = chartReadout(spec, index);
      if (cursor instanceof SVGElement) {
        const x = pointX(index, spec.buckets.length).toFixed(1);
        cursor.setAttribute("x1", x);
        cursor.setAttribute("x2", x);
        cursor.removeAttribute("hidden");
      }
    });
    plot.addEventListener("blur", () => {
      index = -1;
      readout.textContent = "";
      if (cursor instanceof SVGElement) cursor.setAttribute("hidden", "");
    });
  }
}
