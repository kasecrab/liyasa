(function () {
"use strict";

// Everything the dashboard renders is a string, and most of it came from a
// request: a route, a search query, a feedback comment, a version name. There
// is one escaper and every interpolation goes through it.
//
// `html` returns a `Fragment` rather than a string, which is the whole point.
// A fragment nests inside another `html` without being escaped again; a plain
// string never does. So markup composes and data cannot, and neither can be
// mistaken for the other by forgetting a call.

/** Markup that has already been escaped. */
class Fragment {
  // A plain field and an explicit assignment: `readonly value: string` in the
  // parameter list is a TypeScript parameter property, which `build.mjs` and
  // `node --test` both refuse because stripping types cannot produce it.
  value        ;

  constructor(value        ) {
    this.value = value;
  }

  toString()         {
    return this.value;
  }
}

/** For text and attribute values alike; `html` uses it on both. */
function escapeHtml(value         )         {
  return String(value)
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#39;");
}

/**
 * A tagged template that escapes every interpolation.
 *
 * A `Fragment` is inserted as written, an array is joined, `null` and
 * `undefined` are nothing, and everything else is escaped.
 */
function html(strings                      , ...values           )           {
  let out = strings[0] ?? "";
  for (let i = 0; i < values.length; i += 1) {
    out += renderValue(values[i]) + (strings[i + 1] ?? "");
  }
  return new Fragment(out);
}

/**
 * Marks a string as markup.
 *
 * Used where a fragment was assembled by concatenation rather than by `html`,
 * which in this package is the SVG marks and nowhere else.
 */
function raw(value        )           {
  return new Fragment(value);
}

function renderValue(value         )         {
  if (value === null || value === undefined) return "";
  if (value instanceof Fragment) return value.value;
  if (Array.isArray(value)) return value.map(renderValue).join("");
  return escapeHtml(value);
}

// Numbers and dates for a screen. No Intl locale is chosen here: the dashboard
// renders in the operator's browser and `undefined` lets it pick, which is the
// one place a default is better than a decision.

/** `1,204`. */
function formatCount(value        )         {
  return new Intl.NumberFormat(undefined).format(Math.round(value));
}

/** `12%`, and `—` when there is nothing to take a percentage of. */
function formatPercent(value               , digits = 0)         {
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
function formatChange(current        , previous        )         {
  if (previous === 0) return current > 0 ? "new" : "—";
  const change = (current - previous) / previous;
  const sign = change >= 0 ? "+" : "";
  return sign + formatPercent(change);
}

/** `2026-09-14`, in UTC, matching what the API sends. */
function formatDate(ms        )         {
  return new Date(ms).toISOString().slice(0, 10);
}

/** `14 Sep 10:00`, for an hourly axis. */
function formatHour(ms        )         {
  return new Intl.DateTimeFormat(undefined, {
    day: "numeric",
    month: "short",
    hour: "2-digit",
    minute: "2-digit",
    timeZone: "UTC",
  }).format(new Date(ms));
}

/** `1.2 s`, `310 ms`. */
function formatDuration(ms        )         {
  if (ms >= 1000) return `${(ms / 1000).toFixed(1)} s`;
  return `${Math.round(ms)} ms`;
}

/** The label a bucket carries on an axis and in the data table. */
function formatBucket(ms        , grain                )         {
  return grain === "hour" ? formatHour(ms) : formatDate(ms);
}

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


const WIDTH = 720;
const HEIGHT = 180;
const PAD_X = 4;
const PAD_Y = 6;

/** The largest value any series reaches; at least 1, so a flat zero has an axis. */
function chartMax(spec           )         {
  let max = 0;
  for (const series of spec.series) {
    for (const value of series.values) if (value > max) max = value;
  }
  return Math.max(1, max);
}

/** Totals per series, which the legend shows. */
function chartTotals(spec           )                         {
  const totals                         = {};
  for (const series of spec.series) {
    totals[series.key] = series.values.reduce((sum, value) => sum + value, 0);
  }
  return totals;
}

function pointX(index        , count        )         {
  if (count <= 1) return PAD_X;
  return PAD_X + (index * (WIDTH - 2 * PAD_X)) / (count - 1);
}

function pointY(value        , max        )         {
  return HEIGHT - PAD_Y - (value / max) * (HEIGHT - 2 * PAD_Y);
}

/** The `d` of one series' line, rounded so the markup is stable between runs. */
function chartPath(spec           , series             )         {
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
function chartReadout(spec           , index        )         {
  const bucket = spec.buckets[index];
  if (bucket === undefined) return "";
  const parts = spec.series.map(
    (series) => `${series.label} ${formatCount(series.values[index] ?? 0)}`,
  );
  return `${formatBucket(bucket, spec.grain)}: ${parts.join(", ")}`;
}

/** The one-sentence summary the SVG carries as its accessible name. */
function chartSummary(spec           )         {
  const totals = chartTotals(spec);
  const parts = spec.series.map((s) => `${series_total_phrase(s, totals[s.key] ?? 0)}`);
  const span =
    spec.buckets.length > 0
      ? `${formatBucket(spec.buckets[0]          , spec.grain)} to ${formatBucket(
          spec.buckets[spec.buckets.length - 1]          ,
          spec.grain,
        )}`
      : "no data";
  return `${spec.title}, ${span}. ${parts.join(". ")}.`;
}

function series_total_phrase(series             , total        )         {
  return `${series.label}: ${formatCount(total)} total`;
}

/** The table every chart carries, which is also the toggle of ANA-70. */
function chartTable(spec           )           {
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

function renderChart(spec           )           {
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
function attachChartKeys(root            , specs                        )       {
  for (const figure of Array.from(root.querySelectorAll("[data-chart]"))) {
    const id = figure.getAttribute("data-chart") ?? "";
    const spec = specs.get(id);
    const plot = figure.querySelector(".ly-chart-plot");
    const readout = figure.querySelector(".ly-chart-readout");
    const cursor = figure.querySelector(".ly-chart-cursor");
    if (!spec || !plot || !readout) continue;
    let index = -1;
    plot.addEventListener("keydown", (event) => {
      const key = (event                 ).key;
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

// The six filter dimensions of ANA-71, plus the site and environment every
// request is scoped to.
//
// The query-string names and their order are a contract with
// `liyasa_analytics::query::Filters`. `test/filters.fixture.json` is the
// document both sides are held to; neither is checked against the other's
// output.

                                                                   

const CALLER_KINDS               = ["human", "agent", "bot", "integration"];

                          
                
               
                   
                  
                  
                   
                      
                       
 

/** Query-string name against object field, in the canonical order. */
const FILTER_NAMES                                 = [
  ["site", "site"],
  ["env", "env"],
  ["version", "version"],
  ["locale", "locale"],
  ["region", "region"],
  ["product", "product"],
  ["caller", "caller"],
  ["route", "routePrefix"],
];

/** The dimensions a filter bar offers, which is ANA-71's list. */
const FILTER_DIMENSIONS                                                               = [
  { name: "version", field: "version", label: "Version" },
  { name: "locale", field: "locale", label: "Locale" },
  { name: "region", field: "region", label: "Region" },
  { name: "product", field: "product", label: "Product" },
  { name: "caller", field: "caller", label: "Caller" },
];

function filtersToQuery(filters         )         {
  const parameters = new URLSearchParams();
  for (const [name, field] of FILTER_NAMES) {
    const value = filters[field];
    if (value) parameters.append(name, value);
  }
  return parameters.toString();
}

/**
 * Reads a query string back.
 *
 * An unknown name is ignored and an unknown caller kind leaves the filter
 * unset rather than selecting nothing, so a URL bookmarked before a release
 * still shows traffic.
 */
function filtersFromQuery(query        )          {
  const parameters = new URLSearchParams(query.replace(/^\?/, ""));
  const filters          = {};
  for (const [name, field] of FILTER_NAMES) {
    const value = parameters.get(name);
    if (!value) continue;
    if (field === "caller") {
      if ((CALLER_KINDS            ).includes(value)) filters.caller = value              ;
      continue;
    }
    filters[field] = value;
  }
  return filters;
}

/** How many dimensions are set, which is what the filter button counts. */
function activeFilterCount(filters         )         {
  return FILTER_NAMES.filter(([, field]) => Boolean(filters[field])).length;
}

function filtersEqual(a         , b         )          {
  return filtersToQuery(a) === filtersToQuery(b);
}

// The HTTP calls the dashboard makes.
//
// The path list is a contract with `liyasa_analytics::api`, held to
// `test/endpoints.fixture.json` from both sides. Sixteen of these have no
// handler anywhere yet; `servedBy` says so, and a page whose data is not being
// served renders its controls and an explanation rather than an empty chart.

const API_BASE = "/_liyasa/api/v1";

const ENDPOINTS             = [
  { id: "traffic.series", method: "GET", path: `${API_BASE}/analytics/series`, requirement: "ANA-10", servedBy: "unbuilt" },
  { id: "traffic.totals", method: "GET", path: `${API_BASE}/analytics/totals`, requirement: "ANA-10", servedBy: "unbuilt" },
  { id: "traffic.pages", method: "GET", path: `${API_BASE}/analytics/pages`, requirement: "ANA-10", servedBy: "unbuilt" },
  { id: "traffic.referrers", method: "GET", path: `${API_BASE}/analytics/referrers`, requirement: "ANA-10", servedBy: "unbuilt" },
  { id: "traffic.journeys", method: "GET", path: `${API_BASE}/analytics/journeys`, requirement: "ANA-10", servedBy: "unbuilt" },
  { id: "traffic.variants", method: "GET", path: `${API_BASE}/analytics/variants`, requirement: "ANA-10", servedBy: "unbuilt" },
  { id: "traffic.delivery", method: "GET", path: `${API_BASE}/analytics/delivery`, requirement: "ANA-10", servedBy: "unbuilt" },
  { id: "traffic.horizon", method: "GET", path: `${API_BASE}/analytics/horizon`, requirement: "ANA-06", servedBy: "unbuilt" },
  { id: "search.queries", method: "GET", path: `${API_BASE}/analytics/search/queries`, requirement: "ANA-20", servedBy: "unbuilt" },
  { id: "search.pages", method: "GET", path: `${API_BASE}/analytics/search/pages`, requirement: "ANA-20", servedBy: "unbuilt" },
  { id: "search.trending", method: "GET", path: `${API_BASE}/analytics/search/trending`, requirement: "ANA-20", servedBy: "unbuilt" },
  { id: "assistant.summary", method: "GET", path: `${API_BASE}/analytics/assistant`, requirement: "ANA-10", servedBy: "unbuilt" },
  { id: "insights.cards", method: "GET", path: `${API_BASE}/analytics/insights`, requirement: "ANA-40", servedBy: "unbuilt" },
  { id: "insights.act", method: "POST", path: `${API_BASE}/analytics/insights/act`, requirement: "ANA-40", servedBy: "unbuilt" },
  { id: "settings.integrations", method: "GET", path: `${API_BASE}/analytics/integrations`, requirement: "ANA-60", servedBy: "unbuilt" },
  { id: "content.tree", method: "GET", path: `${API_BASE}/content`, requirement: "REST-02", servedBy: "wp-14" },
  { id: "feedback.list", method: "GET", path: "/_liyasa/feedback", requirement: "ANA-30", servedBy: "wp-14" },
  { id: "feedback.summary", method: "GET", path: "/_liyasa/feedback/summary", requirement: "ANA-30", servedBy: "wp-14" },
  { id: "feedback.status", method: "PATCH", path: "/_liyasa/feedback/{id}", requirement: "ANA-30", servedBy: "wp-14" },
  { id: "jobs.list", method: "GET", path: `${API_BASE}/jobs`, requirement: "HOST-07", servedBy: "wp-14" },
  { id: "jobs.retry", method: "POST", path: `${API_BASE}/jobs/{id}/retry`, requirement: "HOST-07", servedBy: "wp-14" },
  { id: "jobs.cancel", method: "POST", path: `${API_BASE}/jobs/{id}/cancel`, requirement: "HOST-07", servedBy: "wp-14" },
  { id: "deployments.list", method: "GET", path: `${API_BASE}/deployments`, requirement: "REST-01", servedBy: "wp-14" },
  { id: "deployments.current", method: "GET", path: `${API_BASE}/deployments/{env}`, requirement: "REST-01", servedBy: "wp-14" },
  { id: "builds.trigger", method: "POST", path: `${API_BASE}/builds`, requirement: "GIT-21", servedBy: "wp-16" },
  { id: "builds.queue", method: "GET", path: `${API_BASE}/builds`, requirement: "GIT-24", servedBy: "wp-16" },
  { id: "builds.status", method: "GET", path: `${API_BASE}/builds/{id}`, requirement: "GIT-21", servedBy: "wp-16" },
  { id: "builds.activate", method: "POST", path: `${API_BASE}/builds/{id}/deploy`, requirement: "GIT-21", servedBy: "wp-16" },
  { id: "deployments.history", method: "GET", path: `${API_BASE}/deployments/{env}/history`, requirement: "GIT-21", servedBy: "wp-16" },
  { id: "deployments.retained", method: "GET", path: `${API_BASE}/deployments/{env}/retained`, requirement: "GIT-40", servedBy: "wp-16" },
  { id: "deployments.rollback", method: "POST", path: `${API_BASE}/deployments/{env}/rollback/{buildId}`, requirement: "GIT-40", servedBy: "wp-16" },
  { id: "deployments.latest", method: "POST", path: `${API_BASE}/deployments/{env}/latest`, requirement: "GIT-41", servedBy: "wp-16" },
  { id: "drift.open", method: "GET", path: `${API_BASE}/drift`, requirement: "REST-05", servedBy: "unbuilt" },
  { id: "proposals.list", method: "GET", path: `${API_BASE}/proposals`, requirement: "REST-02", servedBy: "unbuilt" },
];

function findEndpoint(id        )                       {
  return ENDPOINTS.find((endpoint) => endpoint.id === id);
}

/** Fills `{name}` holes, encoding each value. */
function endpointPath(id        , parameters                         = {})         {
  const endpoint = findEndpoint(id);
  if (!endpoint) throw new Error(`no endpoint \`${id}\``);
  return endpoint.path.replace(/\{(\w+)\}/g, (_match, name        ) => {
    const value = parameters[name];
    if (value === undefined) throw new Error(`\`${id}\` needs a \`${name}\``);
    return encodeURIComponent(value);
  });
}

                            
                
                
                    
                    
                                                      
 

/** The URL for a read, with the range, grain and ANA-71 filters on it. */
function endpointUrl(
  id        ,
  spec            = {},
  parameters                         = {},
)         {
  const search = new URLSearchParams();
  if (spec.range) {
    search.set("from", String(spec.range.from));
    search.set("to", String(spec.range.to));
  }
  if (spec.grain) search.set("grain", spec.grain);
  if (spec.compare) search.set("compare", "1");
  for (const [name, value] of Object.entries(spec.extra ?? {})) {
    if (value !== undefined) search.set(name, String(value));
  }
  const filters = spec.filters ? filtersToQuery(spec.filters) : "";
  const query = [search.toString(), filters].filter(Boolean).join("&");
  const path = endpointPath(id, parameters);
  return query ? `${path}?${query}` : path;
}

                          
                 
                
                  
 

                                                                                 

/**
 * One read.
 *
 * An endpoint nobody serves yet fails without a request: a 404 from a path
 * that was never wired reads like an outage, and this says what it is.
 */
async function read   (
  id        ,
  spec            = {},
  parameters                         = {},
  fetcher               = fetch,
)                     {
  const endpoint = findEndpoint(id);
  if (!endpoint) return { ok: false, problem: { status: 0, title: `no endpoint \`${id}\`` } };
  if (endpoint.servedBy === "unbuilt") {
    return {
      ok: false,
      problem: {
        status: 501,
        title: "Not served yet",
        detail: `${endpoint.requirement}: no handler answers ${endpoint.path} in this build`,
      },
    };
  }
  try {
    const response = await fetcher(endpointUrl(id, spec, parameters), {
      headers: { accept: "application/json" },
    });
    if (!response.ok) {
      const body = (await response.json().catch(() => ({})))                           ;
      return {
        ok: false,
        problem: {
          status: response.status,
          title: typeof body["title"] === "string" ? body["title"] : response.statusText,
          detail: typeof body["detail"] === "string" ? body["detail"] : undefined,
        },
      };
    }
    return { ok: true, value: (await response.json())      };
  } catch (error) {
    return { ok: false, problem: { status: 0, title: "Request failed", detail: String(error) } };
  }
}

// Date ranges, grains and period-over-period comparison (ANA-71).
//
// A range is half-open: `[from, to)`. The end of one period is the start of
// the next, so an event is counted once whichever of the two you ask about.

const HOUR_MS = 3_600_000;
const DAY_MS = 86_400_000;

                                   

                        
               
             
 

                                                                                                       

/** The presets the date picker offers. */
const RANGE_PRESETS                                                     = [
  { id: "today", label: "Today", days: 1 },
  { id: "7d", label: "Last 7 days", days: 7 },
  { id: "28d", label: "Last 28 days", days: 28 },
  { id: "90d", label: "Last 90 days", days: 90 },
  { id: "12m", label: "Last 12 months", days: 365 },
];

function makeRange(from        , to        )        {
  return { from: Math.min(from, to), to: Math.max(from, to) };
}

/**
 * The last `days` whole UTC days ending at the midnight after `now`.
 *
 * Whole days rather than a rolling window from the current instant: "last 7
 * days" compared against "the 7 days before that" is only a comparison if both
 * cover the same hours of the week.
 */
function lastDays(now        , days        )        {
  const end = now - modulo(now, DAY_MS) + DAY_MS;
  return makeRange(end - Math.max(1, days) * DAY_MS, end);
}

function resolveRange(spec           , now        )        {
  return spec.kind === "last" ? lastDays(now, spec.days) : makeRange(spec.from, spec.to);
}

function rangeSpan(range       )         {
  return range.to - range.from;
}

/** The window immediately before this one, for period over period. */
function previousRange(range       )        {
  const span = rangeSpan(range);
  return makeRange(range.from - span, range.from);
}

function grainMillis(grain       )         {
  return grain === "hour" ? HOUR_MS : DAY_MS;
}

/** Hours up to two days, days beyond: 8,760 points is not a chart. */
function grainFor(range       )        {
  return rangeSpan(range) <= 2 * DAY_MS ? "hour" : "day";
}

/** Every bucket boundary the range covers, so a quiet hour is a zero. */
function bucketsOf(range       , grain       )           {
  const step = grainMillis(grain);
  const out           = [];
  for (let at = range.from - modulo(range.from, step); at < range.to; at += step) out.push(at);
  return out;
}

function describeRange(range       )         {
  const days = Math.round(rangeSpan(range) / DAY_MS);
  const preset = RANGE_PRESETS.find((p) => p.days === days);
  return preset ? preset.label : `${new Date(range.from).toISOString().slice(0, 10)} to ${new Date(range.to - 1).toISOString().slice(0, 10)}`;
}

/** `%` is remainder in JavaScript, not modulo; a negative instant needs this. */
function modulo(value        , by        )         {
  return ((value % by) + by) % by;
}

// The eleven pages of ANA-70, and the hash router over them.
//
// A hash rather than the history API: the dashboard is served from one path by
// a server whose fallback route serves the site, so a deep link under a real
// path would have to be routed there too — a change in another package for no
// gain to anybody.


/** ANA-70's list, in the order the requirement gives them. */
const PAGES           = [
  { id: "overview", label: "Overview", summary: "Traffic, score, drift and the review queue in one screen." },
  { id: "traffic", label: "Traffic", summary: "Who read what, split by reader and agent." },
  { id: "search", label: "Search", summary: "What people looked for and whether they found it." },
  { id: "assistant", label: "Assistant", summary: "Questions asked, answers rated, and what it could not answer." },
  { id: "feedback", label: "Feedback", summary: "Ratings and written feedback, with a status workflow." },
  { id: "truth", label: "Truth", summary: "Verification results and open drift." },
  { id: "proposals", label: "Proposals", summary: "The review queue of agent-written changes." },
  { id: "deployments", label: "Deployments", summary: "Builds, the queue, and rollback." },
  { id: "automations", label: "Automations", summary: "Scheduled and triggered jobs." },
  { id: "content", label: "Content", summary: "The page tree and its health." },
  { id: "settings", label: "Settings", summary: "Analytics, retention, integrations and consent." },
];

const DEFAULT_PAGE = "overview";

function isPage(id        )          {
  return PAGES.some((page) => page.id === id);
}

function pageById(id        )                     {
  return PAGES.find((page) => page.id === id);
}

/** Everything a hash carries. */
                             
               
                       
                   
                   
                
                                                                                   
                 
 

function parseRoute(hash        )             {
  const [path = "", query = ""] = hash.replace(/^#\/?/, "").split("?");
  const page = isPage(path) ? path : DEFAULT_PAGE;
  const parameters = new URLSearchParams(query);
  const days = Number(parameters.get("days"));
  const from = Number(parameters.get("from"));
  const to = Number(parameters.get("to"));
  let rangeSpec            = { kind: "last", days: 28 };
  if (Number.isFinite(days) && days > 0) rangeSpec = { kind: "last", days };
  else if (Number.isFinite(from) && Number.isFinite(to) && to > from) {
    rangeSpec = { kind: "between", from, to };
  }
  const grain = parameters.get("grain");
  const focus = parameters.get("focus");
  const state             = {
    page,
    rangeSpec,
    filters: filtersFromQuery(query),
    compare: parameters.get("compare") === "1",
  };
  if (grain === "hour" || grain === "day") state.grain = grain;
  if (focus) state.focus = focus;
  return state;
}

function routeRange(state            , now        )        {
  return resolveRange(state.rangeSpec, now);
}

/** The inverse, so every control on the page can produce a link. */
function routeHref(state            )         {
  const parameters = new URLSearchParams();
  if (state.rangeSpec.kind === "last") parameters.set("days", String(state.rangeSpec.days));
  else {
    parameters.set("from", String(state.rangeSpec.from));
    parameters.set("to", String(state.rangeSpec.to));
  }
  if (state.compare) parameters.set("compare", "1");
  if (state.grain) parameters.set("grain", state.grain);
  if (state.focus) parameters.set("focus", state.focus);
  for (const [name, value] of Object.entries(state.filters)) {
    if (!value) continue;
    parameters.set(name === "routePrefix" ? "route" : name, value);
  }
  const query = parameters.toString();
  return `#/${state.page}${query ? `?${query}` : ""}`;
}

// Saved views (ANA-71).
//
// A view stores a range SPEC rather than two instants: "last 28 days" saved in
// September should still mean the last 28 days in November. A view that wants
// a fixed fortnight says `between` and gets one.
//
// Storage is `localStorage` under one key. It is per browser by design at this
// stage — a server-side view belongs to the REST API and to an account, and
// neither is this package's — and every read is guarded, because a browser in
// private mode throws rather than returning null.


const VIEWS_KEY = "liyasa.dashboard.views";

                            
             
               
               
                   
                   
                   
                
 

                            
                                      
                                            
 

function resolveView(view           , now        )        {
  return resolveRange(view.range, now);
}

/** Reads the saved views, treating anything unreadable as none. */
function loadViews(store                       )              {
  if (!store) return [];
  let text                = null;
  try {
    text = store.getItem(VIEWS_KEY);
  } catch {
    return [];
  }
  if (!text) return [];
  try {
    const parsed          = JSON.parse(text);
    return Array.isArray(parsed) ? parsed.filter(isView) : [];
  } catch {
    return [];
  }
}

function saveViews(store                       , views             )       {
  if (!store) return;
  try {
    store.setItem(VIEWS_KEY, JSON.stringify(views));
  } catch {
    // A full or disabled store loses the view, and losing a bookmark is not
    // worth failing the page over.
  }
}

/** Adds a view, replacing one of the same name on the same page. */
function upsertView(views             , view           )              {
  const without = views.filter((v) => !(v.page === view.page && v.name === view.name));
  return [...without, view];
}

function removeView(views             , id        )              {
  return views.filter((view) => view.id !== id);
}

/** The URL fragment a view opens, which is also what a shared link carries. */
function viewHref(view           )         {
  const search = new URLSearchParams();
  if (view.range.kind === "last") {
    search.set("days", String(view.range.days));
  } else {
    search.set("from", String(view.range.from));
    search.set("to", String(view.range.to));
  }
  if (view.compare) search.set("compare", "1");
  if (view.grain) search.set("grain", view.grain);
  const filters = filtersToQuery(view.filters);
  const query = [search.toString(), filters].filter(Boolean).join("&");
  return `#/${view.page}${query ? `?${query}` : ""}`;
}

/** The inverse, for reading a link someone pasted into a chat. */
function viewFromHref(href        , name = "Shared view")            {
  const [path, query = ""] = href.replace(/^#\/?/, "").split("?");
  const parameters = new URLSearchParams(query);
  const days = Number(parameters.get("days"));
  const from = Number(parameters.get("from"));
  const to = Number(parameters.get("to"));
  const range            =
    Number.isFinite(days) && days > 0
      ? { kind: "last", days }
      : Number.isFinite(from) && Number.isFinite(to) && to > 0
        ? { kind: "between", from, to }
        : { kind: "last", days: 28 };
  const grain = parameters.get("grain");
  const view            = {
    id: `v-${path || "overview"}-${query.length}`,
    name,
    page: path || "overview",
    range,
    filters: filtersFromQuery(query),
    compare: parameters.get("compare") === "1",
  };
  if (grain === "hour" || grain === "day") view.grain = grain;
  return view;
}

function isView(value         )                     {
  if (typeof value !== "object" || value === null) return false;
  const candidate = value                           ;
  return (
    typeof candidate["id"] === "string" &&
    typeof candidate["name"] === "string" &&
    typeof candidate["page"] === "string" &&
    typeof candidate["range"] === "object" &&
    candidate["range"] !== null
  );
}

// The toolbar every page carries (ANA-71): the date range, the
// period-over-period toggle, the six filters, and the saved views.
//
// Each control is a link rather than a form. A link is shareable, works before
// any script runs, restores on a back button, and is what a saved view stores;
// a `<select>` that only works once `dashboard.js` has loaded is none of those.





/** What the filter bar offers for each dimension, from the data on screen. */
                                
                     
                    
                    
                     
                    
 

function renderRangePicker(state            , range       )           {
  const current =
    state.rangeSpec.kind === "last" ? state.rangeSpec.days : Math.round((range.to - range.from) / 86_400_000);
  return html`<fieldset class="ly-control ly-range">
    <legend>Range</legend>
    ${RANGE_PRESETS.map((preset) => {
      const href = routeHref({ ...state, rangeSpec: { kind: "last", days: preset.days } });
      const active = state.rangeSpec.kind === "last" && state.rangeSpec.days === preset.days;
      return html`<a href="${href}" aria-current="${active ? "true" : "false"}">${preset.label}</a>`;
    })}
    <span class="ly-range-current">${describeRange(range)}</span>
    <span class="ly-visually-hidden">${String(current)} days</span>
  </fieldset>`;
}

function renderCompareToggle(state            )           {
  const href = routeHref({ ...state, compare: !state.compare });
  return html`<a
    class="ly-control ly-compare"
    href="${href}"
    role="switch"
    aria-checked="${state.compare ? "true" : "false"}"
    >Compare with the previous period</a
  >`;
}

function renderFilterBar(state            , options               )           {
  const count = activeFilterCount(state.filters);
  const groups = FILTER_DIMENSIONS.map((dimension) => {
    const values = options[dimension.name                       ] ?? [];
    const chosen = state.filters[dimension.field];
    if (values.length === 0 && !chosen) return null;
    const items = values.map((value) => {
      const next          = { ...state.filters, [dimension.field]: value };
      const active = chosen === value;
      const href = routeHref({
        ...state,
        filters: active ? { ...state.filters, [dimension.field]: undefined } : next,
      });
      return html`<a href="${href}" aria-pressed="${active ? "true" : "false"}">${value}</a>`;
    });
    return html`<div class="ly-filter-group" data-dimension="${dimension.name}">
      <span class="ly-filter-label">${dimension.label}</span>
      ${items}
    </div>`;
  });
  const clear = count > 0 ? html`<a class="ly-filter-clear" href="${routeHref({ ...state, filters: {} })}">Clear ${String(count)}</a>` : null;
  return html`<section class="ly-control ly-filters" aria-label="Filters">
    ${groups}${clear}
  </section>`;
}

function renderSavedViews(state            , views             )           {
  const forPage = views.filter((view) => view.page === state.page);
  return html`<section class="ly-control ly-views" aria-label="Saved views">
    <span class="ly-filter-label">Saved</span>
    ${forPage.length === 0
      ? html`<span class="ly-empty">none yet</span>`
      : forPage.map(
          (view) =>
            html`<a href="${viewHref(view)}" data-view="${view.id}">${view.name}</a
              ><button type="button" data-remove-view="${view.id}" aria-label="Delete ${view.name}">
                ×
              </button>`,
        )}
    <button type="button" data-save-view="${state.page}">Save this view</button>
  </section>`;
}

function renderToolbar(
  state            ,
  range       ,
  options               ,
  views             ,
)           {
  return html`<div class="ly-toolbar">
    ${renderRangePicker(state, range)} ${renderCompareToggle(state)}
    ${renderFilterBar(state, options)} ${renderSavedViews(state, views)}
  </div>`;
}

/** The navigation, which is eleven links and nothing clever. */
function renderNav(state            , pages                                      )           {
  return html`<nav class="ly-nav" aria-label="Dashboard">
    <ul>
      ${pages.map((page) => {
        const href = routeHref({ ...state, page: page.id, focus: undefined });
        const current = page.id === state.page;
        return html`<li>
          <a href="${href}" aria-current="${current ? "page" : "false"}">${page.label}</a>
        </li>`;
      })}
    </ul>
  </nav>`;
}

// The eleven pages of ANA-70.
//
// Every renderer is a pure function from state and already-fetched data to a
// `Fragment`. That is the whole architecture: fetching happens in
// `dashboard.ts`, rendering is `innerHTML = String(renderPage(...))`, and a
// test can assert on the output without a browser or a stand-in for one. A
// test against a fake DOM proves the fake DOM works.






/** A number and the same number last period. */
                             
                  
                   
 

                                
               
                           
                           
                                                                                                    
 

                           
                                             
 

/** One headline number. */
function renderStat(label        , value        , change         )           {
  return html`<div class="ly-stat">
    <dt>${label}</dt>
    <dd><b>${value}</b>${change ? html`<span class="ly-stat-change">${change}</span>` : null}</dd>
  </div>`;
}

function renderStats(stats            )           {
  return html`<dl class="ly-stats">${stats}</dl>`;
}

/**
 * What a panel shows when its endpoint answered with a problem.
 *
 * "Not served yet" is spelled out rather than shown as an error, because it is
 * not one: the page is complete and the handler is another package's. An empty
 * chart in its place would say the site had no traffic.
 */
function renderProblem(title        , problem         )           {
  const unserved = problem.status === 501;
  return html`<section class="ly-panel ly-problem" data-unserved="${unserved ? "true" : "false"}">
    <h3>${title}</h3>
    <p class="ly-problem-title">
      ${unserved ? "No data is being served for this yet" : problem.title}
    </p>
    ${problem.detail ? html`<p class="ly-problem-detail">${problem.detail}</p>` : null}
  </section>`;
}

/** Unwraps a result into a panel or an explanation. */
function panel   (
  title        ,
  result                       ,
  draw                        ,
)           {
  if (!result) return renderProblem(title, { status: 0, title: "Not loaded" });
  if (!result.ok) return renderProblem(title, result.problem);
  return html`<section class="ly-panel">
    <h3>${title}</h3>
    ${draw(result.value)}
  </section>`;
}

/** A series payload as a chart, with ANA-10's label when it is client measured. */
function seriesChart(
  id        ,
  title        ,
  payload               ,
  delivery                ,
)            {
  const measured =
    delivery === null || delivery === undefined
      ? ""
      : ` — ${formatPercent(delivery)} of page loads delivered a beacon`;
  const spec            = {
    id,
    title,
    grain: payload.grain,
    kind: "line",
    buckets: payload.points.map((point) => point.bucket),
    series: [
      { key: "human", label: "Readers", values: payload.points.map((p) => p.human) },
      { key: "agent", label: "Agents", values: payload.points.map((p) => p.agent) },
      { key: "bot", label: "Crawlers", values: payload.points.map((p) => p.bot) },
    ],
  };
  if (payload.sampledByClient) spec.note = `Sampled by client${measured}`;
  return spec;
}

/** An empty chart of the right shape, so a page with no data still draws. */
function emptyChart(id        , title        , range       , grain        )            {
  const resolved = grain ?? grainFor(range);
  const buckets = bucketsOf(range, resolved);
  return {
    id,
    title,
    grain: resolved,
    kind: "line",
    buckets,
    series: [{ key: "human", label: "Readers", values: buckets.map(() => 0) }],
  };
}

function renderTable(headers          , body            )           {
  if (body.length === 0) return html`<p class="ly-empty">Nothing over this period.</p>`;
  return html`<table class="ly-table">
    <thead>
      <tr>
        ${headers.map((header) => html`<th scope="col">${header}</th>`)}
      </tr>
    </thead>
    <tbody>
      ${body.map((row) => html`<tr>${row.map((cell) => html`<td>${cell}</td>`)}</tr>`)}
    </tbody>
  </table>`;
}

// ---- the pages ----

                              
               
                
                 
                 
                                   
                                                           
 

function renderInsightList(cards               )           {
  if (cards.length === 0) {
    return html`<p class="ly-empty">Nothing stood out over this period.</p>`;
  }
  const items = cards.map((card) => {
    const action = card.action;
    const button = action
      ? html`<button type="button" data-action="${action.kind}" data-target="${action.target}">
          ${action.label}
        </button>`
      : null;
    return html`<li class="ly-card" data-card="${card.kind}">
      <h4>${card.title}</h4>
      <p>${card.detail}</p>
      ${button}
    </li>`;
  });
  return html`<ul class="ly-cards">
    ${items}
  </ul>`;
}

function renderOverview(state            , data          )           {
  const totals = data["traffic.totals"]                                                  ;
  const series = data["traffic.series"]                                     ;
  const insights = data["insights.cards"]                                                ;
  const stats =
    totals && totals.ok
      ? renderStats(
          Object.entries(totals.value).map(([label, comparison]) =>
            renderStat(
              label,
              formatCount(comparison.current),
              state.compare ? formatChange(comparison.current, comparison.previous) : undefined,
            ),
          ),
        )
      : renderProblem("Headline numbers", problemOf(totals));
  const traffic = panel("Traffic", series, (payload) =>
    renderChart(seriesChart("overview-traffic", "Page views", payload)),
  );
  const cards = panel("What to look at", insights, (value) => renderInsightList(value.cards));
  return html`${stats}${traffic}${cards}`;
}

function problemOf(result                             )          {
  if (result && !result.ok) return result.problem;
  return { status: 0, title: "Not loaded" };
}

                          
                
                
                
              
                      
 

                          
               
                
 

/**
 * ANA-10 asks for the measured beacon delivery ratio next to the client-side
 * series, so an operator never reads them as absolute.
 */
function renderDeliveryNote(ratio                           )                  {
  if (ratio === undefined) return null;
  if (ratio === null) {
    return html`<p class="ly-note">
      No page views over this period, so there is no beacon delivery ratio to measure.
    </p>`;
  }
  return html`<p class="ly-note">
    ${formatPercent(ratio)} of page loads delivered a client beacon. Scroll depth, copy actions and
    tab choices are counted from those, so they undercount by roughly the rest.
  </p>`;
}

function renderTraffic(state            , data          )           {
  const series = data["traffic.series"]                                     ;
  const pages = data["traffic.pages"]                                            ;
  const referrers = data["traffic.referrers"]                                                ;
  const journeys = data["traffic.journeys"]   
                                                   
               ;
  const delivery = data["traffic.delivery"]                                                ;
  const ratio = delivery && delivery.ok ? delivery.value.ratio : undefined;

  const overTime = panel("Over time", series, (payload) =>
    renderChart(seriesChart("traffic-series", "Page views", payload, ratio)),
  );
  const mostRead = panel("Most read", pages, (value) =>
    renderTable(
      ["Page", "Readers", "Agents", "Total"],
      value.pages.map((page) => [
        page.route,
        formatCount(page.human),
        formatCount(page.agent),
        formatCount(page.human + page.agent + page.bot + page.integration),
      ]),
    ),
  );
  const hosts = panel("Referrers", referrers, (value) =>
    renderTable(["Host", "Visits"], value.referrers.map((row) => [row.name, formatCount(row.count)])),
  );
  const journey = panel("Entry and exit pages", journeys, (value) => {
    const entry = renderTable(
      ["Entered on", "Sessions"],
      value.entry.map((row) => [row.name, formatCount(row.count)]),
    );
    const exit = renderTable(
      ["Left from", "Sessions"],
      value.exit.map((row) => [row.name, formatCount(row.count)]),
    );
    return html`${entry}${exit}`;
  });
  const tree = html`<p class="ly-source">
    <a href="${routeHref({ ...state, page: "content" })}">See the page tree</a>
  </p>`;
  return html`${overTime}${mostRead}${hosts}${journey}${renderDeliveryNote(ratio)}${tree}`;
}

                           
            
                   
                
                 
                     
 

                                
                
                      
                 
 

                           
            
                   
                   
 

function renderSearch(_state            , data          )           {
  const queries = data["search.queries"]                                               ;
  const pages = data["search.pages"]                                                  ;
  const trending = data["search.trending"]                                                ;

  const list = panel("Queries", queries, (value) =>
    renderTable(
      ["Query", "Searches", "Click-through", "Top result"],
      value.queries.map((row) => [
        row.q,
        formatCount(row.searches),
        formatPercent(row.searches > 0 ? row.clicks / row.searches : null),
        row.topResult ?? "—",
      ]),
    ),
  );
  const nothing = panel("Found nothing", queries, (value) => {
    const empty = value.queries.filter((row) => row.empty > 0);
    if (empty.length === 0) return html`<p class="ly-empty">Every search found something.</p>`;
    const items = empty.map(
      (row) => html`<li class="ly-card">
        <h4>${row.q}</h4>
        <p>${formatCount(row.empty)} searches returned nothing</p>
        <button type="button" data-action="create_page" data-target="${row.q}">
          Create a page for this
        </button>
      </li>`,
    );
    return html`<ul class="ly-cards">
      ${items}
    </ul>`;
  });
  const perPage = panel("Per page", pages, (value) =>
    renderTable(
      ["Page", "Impressions", "Clicks", "Click-through"],
      value.pages.map((row) => [
        row.route,
        formatCount(row.impressions),
        formatCount(row.clicks),
        formatPercent(row.impressions > 0 ? row.clicks / row.impressions : null),
      ]),
    ),
  );
  const rising = panel("Trending", trending, (value) =>
    renderTable(
      ["Query", "Searches", "Previous", "Change"],
      value.trending.map((row) => [
        row.q,
        formatCount(row.searches),
        formatCount(row.previous),
        formatChange(row.searches, row.previous),
      ]),
    ),
  );
  return html`${list}${nothing}${perPage}${rising}`;
}

function renderAssistant(_state            , data          )           {
  const summary = data["assistant.summary"]   
                                                                                         
               ;
  return panel("Assistant", summary, (value) => {
    const stats = renderStats([
      renderStat("Messages", formatCount(value.messages)),
      renderStat("Rated", formatCount(value.rated)),
      renderStat("Answered well", formatPercent(value.rated > 0 ? value.positive / value.rated : null)),
    ]);
    const gaps = renderTable(["Topic"], value.unanswered.map((topic) => [topic]));
    return html`${stats}
      <h4>Could not answer</h4>
      ${gaps}`;
  });
}

                              
             
                
               
                  
                
                
                 
 

function renderFeedback(state            , data          )           {
  const list = data["feedback.list"]                                                ;
  const summary = data["feedback.summary"]                                                    ;

  const score = panel("Score", summary, (value) => {
    const total = value.up + value.down;
    return renderStats([
      renderStat("Satisfaction", formatPercent(total > 0 ? value.up / total : null)),
      renderStat("Up", formatCount(value.up)),
      renderStat("Down", formatCount(value.down)),
    ]);
  });
  const written = panel("Written feedback", list, (value) => {
    const readers = value.items.filter((item) => item.kind !== "agent");
    const agents = value.items.filter((item) => item.kind === "agent");
    const readerTable = renderTable(
      ["Page", "Rating", "What they said", "Status"],
      readers.map((item) => [
        item.route,
        item.rating === undefined ? "—" : item.rating > 0 ? "up" : "down",
        item.text ?? "—",
        item.status,
      ]),
    );
    const agentTable = renderTable(
      ["Page", "Task it was doing", "Status"],
      agents.map((item) => [item.route, item.task ?? "—", item.status]),
    );
    return html`${readerTable}
      <h4>From agents</h4>
      ${agentTable}
      <p class="ly-note">
        Agent reports are counted beside the score and never inside it: an agent that could not
        finish a task is telling you something different from a reader's thumb.
        <a href="${routeHref({ ...state, page: "truth" })}">Open the drift queue</a>
      </p>`;
  });
  return html`${score}${written}`;
}

                           
                
                
                 
                  
 

function renderTruth(_state            , data          )           {
  const drift = data["drift.open"]                                             ;
  return panel("Open drift", drift, (value) =>
    renderTable(
      ["Page", "Claim", "Source", "Found"],
      value.items.map((row) => [row.route, row.claim, row.source, formatDate(row.foundAt)]),
    ),
  );
}

                              
                
                 
                
                   
 

function renderProposals(_state            , data          )           {
  const proposals = data["proposals.list"]                                                ;
  return panel("Review queue", proposals, (value) =>
    renderTable(
      ["Title", "Author", "Pages", "Opened"],
      value.items.map((row) => [
        row.title,
        row.author,
        formatCount(row.pages),
        formatDate(row.openedAt),
      ]),
    ),
  );
}

                           
                
                   
                   
                                  
 

                             
                
                 
                 
             
 

function renderDeployments(_state            , data          )           {
  const queue = data["builds.queue"]   
                                                                   
               ;
  const history = data["deployments.history"]                                               ;

  const trigger = html`<section class="ly-panel">
    <h3>Deploy</h3>
    <form data-deploy-form>
      <label>Branch <input name="branch" value="main" required /></label>
      <label>Commit <input name="commit" required /></label>
      <button type="submit" data-action="trigger_build">Deploy</button>
    </form>
  </section>`;
  const waiting = panel("Queue", queue, (value) => {
    const stats = renderStats([
      renderStat("Waiting", formatCount(value.depth)),
      renderStat("Running", formatCount(value.running)),
    ]);
    const table = renderTable(
      ["Position", "Project", "Estimated start"],
      value.items.map((row) => [
        String(row.position),
        row.project ?? "—",
        row.estimatedStartMs === null ? "—" : formatDate(row.estimatedStartMs),
      ]),
    );
    return html`${stats}${table}`;
  });
  const past = panel("History", history, (value) =>
    renderTable(
      ["Build", "Branch", "Status", "When"],
      value.items.map((row) => [row.build, row.branch, row.status, formatDate(row.at)]),
    ),
  );
  return html`${trigger}${waiting}${past}`;
}

                         
             
               
                
                   
                    
 

function renderAutomations(_state            , data          )           {
  const jobs = data["jobs.list"]                                           ;
  return panel("Jobs", jobs, (value) =>
    renderTable(
      ["Name", "State", "Attempts", "Updated"],
      value.items.map((row) => [
        row.name,
        row.state,
        formatCount(row.attempts),
        formatDate(row.updatedAt),
      ]),
    ),
  );
}

                             
                
                          
                    
                    
 

function renderContent(_state            , data          )           {
  const tree = data["content.tree"]                                               ;
  return panel("Pages", tree, (value) =>
    renderTable(
      ["Page", "Description", "Updated", "Open drift"],
      value.pages.map((row) => [
        row.route,
        row.hasDescription ? "yes" : "missing",
        formatDate(row.updatedAt),
        formatCount(row.openDrift),
      ]),
    ),
  );
}

                                 
              
               
                  
                              
 

function renderSettings(_state            , data          )           {
  const integrations = data["settings.integrations"]   
              
                                  
                          
                        
                                 
        
               ;
  return panel("Integrations", integrations, (value) => {
    const table = renderTable(
      ["Vendor", "Consent", "Loads before consent"],
      value.enabled.map((row) => [row.name, row.consent, row.loadsBeforeConsent ? "yes" : "no"]),
    );
    const stuck =
      value.stuck.length > 0
        ? html`<p class="ly-warning">
            ${value.stuck.join(", ")} wait for consent and no consent provider is configured, so
            they never load.
          </p>`
        : null;
    return html`${table}${stuck}
      <h4>Consent</h4>
      <p class="ly-note">${value.consentStatement}</p>`;
  });
}

/** Every page's renderer, by id. */
const RENDERERS                                                                  = {
  overview: renderOverview,
  traffic: renderTraffic,
  search: renderSearch,
  assistant: renderAssistant,
  feedback: renderFeedback,
  truth: renderTruth,
  proposals: renderProposals,
  deployments: renderDeployments,
  automations: renderAutomations,
  content: renderContent,
  settings: renderSettings,
};

/** What each page reads, so `dashboard.ts` fetches without a second list. */
const PAGE_ENDPOINTS                           = {
  overview: ["traffic.totals", "traffic.series", "insights.cards"],
  traffic: [
    "traffic.series",
    "traffic.pages",
    "traffic.referrers",
    "traffic.journeys",
    "traffic.variants",
    "traffic.delivery",
  ],
  search: ["search.queries", "search.pages", "search.trending"],
  assistant: ["assistant.summary"],
  feedback: ["feedback.list", "feedback.summary"],
  truth: ["drift.open"],
  proposals: ["proposals.list"],
  deployments: ["builds.queue", "deployments.history"],
  automations: ["jobs.list"],
  content: ["content.tree"],
  settings: ["settings.integrations"],
};

function renderPage(state            , data          )           {
  const renderer = RENDERERS[state.page];
  if (!renderer) return renderProblem("Unknown page", { status: 404, title: state.page });
  return renderer(state, data);
}

/** Endpoints a page needs that nothing serves, for the banner it shows. */
function unservedFor(page        )           {
  return (PAGE_ENDPOINTS[page] ?? []).filter((id) => findEndpoint(id)?.servedBy === "unbuilt");
}

// The entry point: route, fetch, render, wire.
//
// Everything above this file is pure. This is the only module that touches the
// document, the network or storage, and it is deliberately the shortest one.








const CHART_SPECS = new Map                   ();

/** The shell, which is everything except the page body. */
function renderShell(
  state            ,
  body          ,
  views             ,
  options               ,
  nowMs        ,
)           {
  const page = pageById(state.page) ?? pageById(DEFAULT_PAGE);
  const range = routeRange(state, nowMs);
  const unserved = unservedFor(state.page);
  return html`${renderNav(state, PAGES)}
  <main class="ly-main" id="main">
    <header class="ly-page-header">
      <h1>${page?.label ?? state.page}</h1>
      <p>${page?.summary ?? ""}</p>
    </header>
    ${renderToolbar(state, range, options, views)}
    ${unserved.length > 0
      ? html`<p class="ly-warning" data-unserved-count="${String(unserved.length)}">
          ${String(unserved.length)} of this page's data sources have no handler in this build:
          ${unserved.join(", ")}.
        </p>`
      : ""}
    ${body}
  </main>`;
}

async function loadPage(state            , nowMs        )                    {
  const range = routeRange(state, nowMs);
  const grain = state.grain ?? grainFor(range);
  const spec = { range, grain, filters: state.filters, compare: state.compare };
  const ids = PAGE_ENDPOINTS[state.page] ?? [];
  const results = await Promise.all(ids.map((id) => read         (id, spec)));
  const data           = {};
  ids.forEach((id, index) => {
    data[id] = results[index]                   ;
  });
  return data;
}

function currentHash()         {
  return typeof location === "undefined" ? "" : location.hash;
}

async function draw(root         )                {
  const state = parseRoute(currentHash());
  const nowMs = Date.now();
  const views = loadViews(typeof localStorage === "undefined" ? undefined : localStorage);
  let body          ;
  try {
    const data = await loadPage(state, nowMs);
    body = renderPage(state, data);
  } catch (error) {
    body = renderProblem("This page could not be drawn", {
      status: 0,
      title: "Unexpected failure",
      detail: String(error),
    });
  }
  root.innerHTML = String(renderShell(state, body, views, {}, nowMs));
  attachChartKeys(root, CHART_SPECS);
  wire(root, state, views);
}

function wire(root         , state            , views             )       {
  for (const button of Array.from(root.querySelectorAll("[data-save-view]"))) {
    button.addEventListener("click", () => {
      const name = prompt("Name this view");
      if (!name) return;
      const view            = {
        id: `v-${Date.now().toString(36)}`,
        name,
        page: state.page,
        range: state.rangeSpec,
        filters: state.filters,
        compare: state.compare,
      };
      if (state.grain) view.grain = state.grain;
      saveViews(localStorage, upsertView(views, view));
      void draw(root);
    });
  }
  for (const button of Array.from(root.querySelectorAll("[data-remove-view]"))) {
    button.addEventListener("click", () => {
      const id = button.getAttribute("data-remove-view") ?? "";
      saveViews(localStorage, removeView(views, id));
      void draw(root);
    });
  }
}

/** Mounts the dashboard into `root` and follows the hash from there. */
function startDashboard(root         )       {
  if (!location.hash) location.hash = routeHref({ page: DEFAULT_PAGE, rangeSpec: { kind: "last", days: 28 }, filters: {}, compare: false });
  void draw(root);
  addEventListener("hashchange", () => void draw(root));
}

const mount = typeof document === "undefined" ? null : document.getElementById("dashboard");
if (mount) startDashboard(mount);
})();
