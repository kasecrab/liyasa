// The eleven pages of ANA-70.
//
// Every renderer is a pure function from state and already-fetched data to a
// `Fragment`. That is the whole architecture: fetching happens in
// `dashboard.ts`, rendering is `innerHTML = String(renderPage(...))`, and a
// test can assert on the output without a browser or a stand-in for one. A
// test against a fake DOM proves the fake DOM works.

import { html } from "./escape.ts";
import type { Fragment } from "./escape.ts";
import { formatChange, formatCount, formatDate, formatPercent } from "./format.ts";
import { renderChart } from "./chart.ts";
import type { ChartSpec } from "./chart.ts";
import type { Problem, Result } from "./api.ts";
import { findEndpoint } from "./api.ts";
import { routeHref } from "./router.ts";
import type { RouteState } from "./router.ts";
import type { Filters } from "./filters.ts";
import type { Grain, Range } from "./ranges.ts";
import { bucketsOf, grainFor } from "./ranges.ts";

/** A number and the same number last period. */
export interface Comparison {
  current: number;
  previous: number;
}

export interface SeriesPayload {
  grain: Grain;
  source: "rollup" | "raw";
  sampledByClient: boolean;
  points: Array<{ bucket: number; human: number; agent: number; bot: number; integration: number }>;
}

export interface PageData {
  [key: string]: Result<unknown> | undefined;
}

/** One headline number. */
export function renderStat(label: string, value: string, change?: string): Fragment {
  return html`<div class="ly-stat">
    <dt>${label}</dt>
    <dd><b>${value}</b>${change ? html`<span class="ly-stat-change">${change}</span>` : null}</dd>
  </div>`;
}

export function renderStats(stats: Fragment[]): Fragment {
  return html`<dl class="ly-stats">${stats}</dl>`;
}

/**
 * What a panel shows when its endpoint answered with a problem.
 *
 * "Not served yet" is spelled out rather than shown as an error, because it is
 * not one: the page is complete and the handler is another package's. An empty
 * chart in its place would say the site had no traffic.
 */
export function renderProblem(title: string, problem: Problem): Fragment {
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
export function panel<T>(
  title: string,
  result: Result<T> | undefined,
  draw: (value: T) => Fragment,
): Fragment {
  if (!result) return renderProblem(title, { status: 0, title: "Not loaded" });
  if (!result.ok) return renderProblem(title, result.problem);
  return html`<section class="ly-panel">
    <h3>${title}</h3>
    ${draw(result.value)}
  </section>`;
}

/** How far back each table can still answer (ANA-06, RFC 1702). */
export interface Horizon {
  rawFrom: number;
  rollupFrom: number;
}

/**
 * The sentence RFC 1702 promised a chart would carry, when it applies.
 *
 * A filter on version, locale, region or product cannot be answered from the
 * hourly rollup, so the query falls to the raw `event` table — which is kept 90
 * days against the rollup's thirteen months. Ask for a year and the chart draws
 * a confident line over nine months whose rows were DELETED, beside a totals
 * panel for the same period that is right. Two numbers on one page disagreeing
 * with nothing saying why is the failure this note exists to prevent.
 *
 * `None` when it does not apply: the rollup answered, or the chart does not
 * reach past what raw still holds.
 */
export function retentionNote(payload: SeriesPayload, horizon: Horizon | undefined): string | null {
  if (payload.source !== "raw" || !horizon) return null;
  const first = payload.points[0]?.bucket;
  if (first === undefined || first >= horizon.rawFrom) return null;
  return `Reads raw events, which reach back to ${formatDate(horizon.rawFrom)}. Buckets before that are empty because the rows were deleted, not because there was no traffic.`;
}

/** A series payload as a chart, with ANA-10's label when it is client measured. */
export function seriesChart(
  id: string,
  title: string,
  payload: SeriesPayload,
  delivery?: number | null,
  horizon?: Horizon,
): ChartSpec {
  const measured =
    delivery === null || delivery === undefined
      ? ""
      : ` — ${formatPercent(delivery)} of page loads delivered a beacon`;
  const spec: ChartSpec = {
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
  // A chart can be both client-sampled and reading a shortened window, and
  // both matter, so neither replaces the other.
  const notes = [
    payload.sampledByClient ? `Sampled by client${measured}` : null,
    retentionNote(payload, horizon),
  ].filter((note): note is string => note !== null);
  if (notes.length > 0) spec.note = notes.join(" · ");
  return spec;
}

/** An empty chart of the right shape, so a page with no data still draws. */
export function emptyChart(id: string, title: string, range: Range, grain?: Grain): ChartSpec {
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

export function renderTable(headers: string[], body: string[][], empty: string): Fragment {
  // Required rather than defaulted. One shared default is what made this
  // wrong: "Nothing over this period." was correct where it was written and
  // false at every call site whose source is a current set rather than a
  // window, and nothing could see the difference.
  if (typeof empty !== "string" || empty.trim() === "") {
    throw new Error("renderTable needs an empty-state sentence about its own rows");
  }
  if (body.length === 0) return html`<p class="ly-empty">${empty}</p>`;
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

export interface InsightCard {
  kind: string;
  title: string;
  detail: string;
  route?: string;
  metrics: Record<string, unknown>;
  action?: { kind: string; label: string; target: string };
}

export function renderInsightList(cards: InsightCard[]): Fragment {
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

export function renderOverview(state: RouteState, data: PageData): Fragment {
  const totals = data["traffic.totals"] as Result<Record<string, Comparison>> | undefined;
  const series = data["traffic.series"] as Result<SeriesPayload> | undefined;
  const insights = data["insights.cards"] as Result<{ cards: InsightCard[] }> | undefined;
  const reach = data["traffic.horizon"] as Result<Horizon> | undefined;
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
    renderChart(
      seriesChart(
        "overview-traffic",
        "Page views",
        payload,
        undefined,
        reach && reach.ok ? reach.value : undefined,
      ),
    ),
  );
  const cards = panel("What to look at", insights, (value) => renderInsightList(value.cards));
  return html`${stats}${traffic}${cards}`;
}

function problemOf(result: Result<unknown> | undefined): Problem {
  if (result && !result.ok) return result.problem;
  return { status: 0, title: "Not loaded" };
}

export interface PageRow {
  route: string;
  human: number;
  agent: number;
  bot: number;
  integration: number;
}

export interface NameRow {
  name: string;
  count: number;
}

/**
 * ANA-10 asks for the measured beacon delivery ratio next to the client-side
 * series, so an operator never reads them as absolute.
 */
export function renderDeliveryNote(ratio: number | null | undefined): Fragment | null {
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

/** ANA-10's four `variant` dimensions, as the API returns them. */
export interface VariantSplit {
  version: NameRow[];
  locale: NameRow[];
  region: NameRow[];
  product: NameRow[];
}

/**
 * The version, locale, region and product splits (ANA-10).
 *
 * Each is a link that sets the matching filter, because the answer to "who is
 * still on v1" is always followed by "show me only them". `routeHref` is what
 * makes that a shareable URL rather than a click nobody else can repeat.
 */
export function renderVariants(state: RouteState, split: VariantSplit): Fragment {
  const dimensions: Array<[keyof VariantSplit, string, keyof Filters]> = [
    ["version", "Version", "version"],
    ["locale", "Locale", "locale"],
    ["region", "Region", "region"],
    ["product", "Product", "product"],
  ];
  const groups = dimensions.map(([key, label, field]) => {
    const rows = split[key] ?? [];
    if (rows.length === 0) return null;
    const total = rows.reduce((sum, row) => sum + row.count, 0);
    return html`<div class="ly-variant" data-dimension="${key}">
      <h4>${label}</h4>
      <ul class="ly-variant-list">
        ${rows.map((row) => {
          const chosen = state.filters[field] === row.name;
          const href = routeHref({
            ...state,
            filters: { ...state.filters, [field]: chosen ? undefined : row.name },
          });
          return html`<li>
            <a href="${href}" aria-pressed="${chosen ? "true" : "false"}">${row.name}</a>
            <b>${formatCount(row.count)}</b>
            <span class="ly-variant-share"
              >${formatPercent(total > 0 ? row.count / total : null)}</span
            >
          </li>`;
        })}
      </ul>
    </div>`;
  });
  if (groups.every((group) => group === null)) {
    return html`<p class="ly-empty">
      No version, locale, region or product was recorded over this period.
    </p>`;
  }
  return html`<div class="ly-variants">${groups}</div>`;
}

export function renderTraffic(state: RouteState, data: PageData): Fragment {
  const series = data["traffic.series"] as Result<SeriesPayload> | undefined;
  const splits = data["traffic.variants"] as Result<VariantSplit> | undefined;
  const pages = data["traffic.pages"] as Result<{ pages: PageRow[] }> | undefined;
  const referrers = data["traffic.referrers"] as Result<{ referrers: NameRow[] }> | undefined;
  const journeys = data["traffic.journeys"] as
    | Result<{ entry: NameRow[]; exit: NameRow[] }>
    | undefined;
  const delivery = data["traffic.delivery"] as Result<{ ratio: number | null }> | undefined;
  const ratio = delivery && delivery.ok ? delivery.value.ratio : undefined;
  const reach = data["traffic.horizon"] as Result<Horizon> | undefined;
  const horizon = reach && reach.ok ? reach.value : undefined;

  const overTime = panel("Over time", series, (payload) =>
    renderChart(seriesChart("traffic-series", "Page views", payload, ratio, horizon)),
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
      "No pages were read over this period.",
    ),
  );
  const hosts = panel("Referrers", referrers, (value) =>
    renderTable(["Host", "Visits"], value.referrers.map((row) => [row.name, formatCount(row.count)]),
      "No referrers over this period.",
    ),
  );
  const journey = panel("Entry and exit pages", journeys, (value) => {
    const entry = renderTable(
      ["Entered on", "Sessions"],
      value.entry.map((row) => [row.name, formatCount(row.count)]),
      "No session began on a page over this period.",
    );
    const exit = renderTable(
      ["Left from", "Sessions"],
      value.exit.map((row) => [row.name, formatCount(row.count)]),
      "No session ended on a page over this period.",
    );
    return html`${entry}${exit}`;
  });
  const variants = panel("Version, locale, region and product", splits, (value) =>
    renderVariants(state, value),
  );
  const tree = html`<p class="ly-source">
    <a href="${routeHref({ ...state, page: "content" })}">See the page tree</a>
  </p>`;
  return html`${overTime}${mostRead}${variants}${hosts}${journey}${renderDeliveryNote(ratio)}${tree}`;
}

export interface QueryRow {
  q: string;
  searches: number;
  empty: number;
  clicks: number;
  topResult?: string;
}

export interface SearchPageRow {
  route: string;
  impressions: number;
  clicks: number;
}

export interface TrendRow {
  q: string;
  searches: number;
  previous: number;
}

export function renderSearch(_state: RouteState, data: PageData): Fragment {
  const queries = data["search.queries"] as Result<{ queries: QueryRow[] }> | undefined;
  const pages = data["search.pages"] as Result<{ pages: SearchPageRow[] }> | undefined;
  const trending = data["search.trending"] as Result<{ trending: TrendRow[] }> | undefined;

  const list = panel("Queries", queries, (value) =>
    renderTable(
      ["Query", "Searches", "Click-through", "Top result"],
      value.queries.map((row) => [
        row.q,
        formatCount(row.searches),
        formatPercent(row.searches > 0 ? row.clicks / row.searches : null),
        row.topResult ?? "—",
      ]),
      "No searches over this period.",
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
      "No page recorded a search over this period.",
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
      "No query trended over this period.",
    ),
  );
  return html`${list}${nothing}${perPage}${rising}`;
}

export function renderAssistant(_state: RouteState, data: PageData): Fragment {
  const summary = data["assistant.summary"] as
    | Result<{ messages: number; rated: number; positive: number; unanswered: string[] }>
    | undefined;
  return panel("Assistant", summary, (value) => {
    const stats = renderStats([
      renderStat("Messages", formatCount(value.messages)),
      renderStat("Rated", formatCount(value.rated)),
      renderStat("Answered well", formatPercent(value.rated > 0 ? value.positive / value.rated : null)),
    ]);
    const gaps = renderTable(["Topic"], value.unanswered.map((topic) => [topic]),
      "No unanswered topics over this period.",
    );
    return html`${stats}
      <h4>Could not answer</h4>
      ${gaps}`;
  });
}

export interface FeedbackRow {
  id: string;
  route: string;
  kind: string;
  rating?: number;
  text?: string;
  task?: string;
  status: string;
}

/** One bucket of `feedback.ratings`. */
export interface RatingPoint {
  bucket: number;
  up: number;
  down: number;
}

export interface RatingSeries {
  grain: Grain;
  route: string | null;
  points: RatingPoint[];
}

/** A page's standing, from `feedback.pages`. */
export interface RatedPage {
  route: string;
  up: number;
  down: number;
  agentReports: number;
  open: number;
}

/**
 * ANA-30's "per-page ratings over time", as a chart of the same shape the
 * traffic series uses so the two read alike.
 *
 * Up and down rather than a single score line: a page that went from two votes
 * to two hundred at the same ratio is a different story from one that did not,
 * and a score line hides the denominator.
 */
export function ratingsChart(id: string, series: RatingSeries): ChartSpec {
  return {
    id,
    title: series.route ? `Ratings for ${series.route}` : "Ratings, site-wide",
    grain: series.grain,
    kind: "bar",
    buckets: series.points.map((point) => point.bucket),
    series: [
      { key: "up", label: "Helpful", values: series.points.map((p) => p.up) },
      { key: "down", label: "Not helpful", values: series.points.map((p) => p.down) },
    ],
  };
}

/**
 * The pages worth looking at first (ANA-30).
 *
 * Agent reports are a column of their own and never folded into the score: an
 * agent that could not finish a task is reporting something a reader's thumb
 * does not. A page with no votes shows no score rather than 0%.
 */
export function renderRatedPages(state: RouteState, pages: RatedPage[]): Fragment {
  if (pages.length === 0) return html`<p class="ly-empty">Nothing rated over this period.</p>`;
  const rows = pages.map((page) => {
    const votes = page.up + page.down;
    const href = routeHref({ ...state, focus: page.route });
    return html`<tr>
      <td><a href="${href}">${page.route}</a></td>
      <td>${votes > 0 ? formatPercent(page.up / votes) : "—"}</td>
      <td>${formatCount(page.up)}</td>
      <td>${formatCount(page.down)}</td>
      <td>${formatCount(page.agentReports)}</td>
      <td>${formatCount(page.open)}</td>
    </tr>`;
  });
  return html`<table class="ly-table">
    <thead>
      <tr>
        <th scope="col">Page</th>
        <th scope="col">Score</th>
        <th scope="col">Helpful</th>
        <th scope="col">Not helpful</th>
        <th scope="col">Agent reports</th>
        <th scope="col">Open</th>
      </tr>
    </thead>
    <tbody>
      ${rows}
    </tbody>
  </table>`;
}

export function renderFeedback(state: RouteState, data: PageData): Fragment {
  const list = data["feedback.list"] as Result<{ items: FeedbackRow[] }> | undefined;
  const summary = data["feedback.summary"] as Result<{ up: number; down: number }> | undefined;
  const ratings = data["feedback.ratings"] as Result<RatingSeries> | undefined;
  const rated = data["feedback.pages"] as Result<{ pages: RatedPage[] }> | undefined;

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
      "No reader left written feedback over this period.",
    );
    const agentTable = renderTable(
      ["Page", "Task it was doing", "Status"],
      agents.map((item) => [item.route, item.task ?? "—", item.status]),
      "No agent left feedback over this period.",
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
  const overTime = panel("Ratings over time", ratings, (value) =>
    renderChart(ratingsChart("feedback-ratings", value)),
  );
  const byPage = panel("By page", rated, (value) => renderRatedPages(state, value.pages));
  return html`${score}${overTime}${byPage}${written}`;
}

export interface DriftRow {
  route: string;
  claim: string;
  source: string;
  foundAt: number;
}

export function renderTruth(_state: RouteState, data: PageData): Fragment {
  const drift = data["drift.open"] as Result<{ items: DriftRow[] }> | undefined;
  return panel("Open drift", drift, (value) =>
    renderTable(
      ["Page", "Claim", "Source", "Found"],
      value.items.map((row) => [row.route, row.claim, row.source, formatDate(row.foundAt)]),
      "No open drift records. An empty list means none are recorded, not that these pages were verified.",
    ),
  );
}

export interface ProposalRow {
  title: string;
  author: string;
  pages: number;
  openedAt: number;
}

export function renderProposals(_state: RouteState, data: PageData): Fragment {
  const proposals = data["proposals.list"] as Result<{ items: ProposalRow[] }> | undefined;
  return panel("Review queue", proposals, (value) =>
    renderTable(
      ["Title", "Author", "Pages", "Opened"],
      value.items.map((row) => [
        row.title,
        row.author,
        formatCount(row.pages),
        formatDate(row.openedAt),
      ]),
      "No proposals are open.",
    ),
  );
}

export interface QueueRow {
  jobId: string;
  project?: string;
  position: number;
  estimatedStartMs: number | null;
}

export interface HistoryRow {
  build: string;
  branch: string;
  status: string;
  at: number;
}

export function renderDeployments(_state: RouteState, data: PageData): Fragment {
  const queue = data["builds.queue"] as
    | Result<{ items: QueueRow[]; depth: number; running: number }>
    | undefined;
  const history = data["deployments.history"] as Result<{ items: HistoryRow[] }> | undefined;

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
      "No builds are waiting.",
    );
    return html`${stats}${table}`;
  });
  const past = panel("History", history, (value) =>
    renderTable(
      ["Build", "Branch", "Status", "When"],
      value.items.map((row) => [row.build, row.branch, row.status, formatDate(row.at)]),
      "No build has run.",
    ),
  );
  return html`${trigger}${waiting}${past}`;
}

export interface JobRow {
  id: string;
  name: string;
  state: string;
  attempts: number;
  updatedAt: number;
}

export function renderAutomations(_state: RouteState, data: PageData): Fragment {
  const jobs = data["jobs.list"] as Result<{ items: JobRow[] }> | undefined;
  return panel("Jobs", jobs, (value) =>
    renderTable(
      ["Name", "State", "Attempts", "Updated"],
      value.items.map((row) => [
        row.name,
        row.state,
        formatCount(row.attempts),
        formatDate(row.updatedAt),
      ]),
      "No jobs are recorded.",
    ),
  );
}

export interface ContentRow {
  route: string;
  hasDescription: boolean;
  updatedAt: number;
  openDrift: number;
}

export function renderContent(_state: RouteState, data: PageData): Fragment {
  const tree = data["content.tree"] as Result<{ pages: ContentRow[] }> | undefined;
  return panel("Pages", tree, (value) =>
    renderTable(
      ["Page", "Description", "Updated", "Open drift"],
      value.pages.map((row) => [
        row.route,
        row.hasDescription ? "yes" : "missing",
        formatDate(row.updatedAt),
        formatCount(row.openDrift),
      ]),
      "This build knows of no pages.",
    ),
  );
}

export interface IntegrationRow {
  key: string;
  name: string;
  consent: string;
  loadsBeforeConsent: boolean;
}

export function renderSettings(_state: RouteState, data: PageData): Fragment {
  const integrations = data["settings.integrations"] as
    | Result<{
        enabled: IntegrationRow[];
        provider?: string;
        stuck: string[];
        consentStatement: string;
      }>
    | undefined;
  return panel("Integrations", integrations, (value) => {
    const table = renderTable(
      ["Vendor", "Consent", "Loads before consent"],
      value.enabled.map((row) => [row.name, row.consent, row.loadsBeforeConsent ? "yes" : "no"]),
      "No integrations are configured.",
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
export const RENDERERS: Record<string, (state: RouteState, data: PageData) => Fragment> = {
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
export const PAGE_ENDPOINTS: Record<string, string[]> = {
  overview: ["traffic.totals", "traffic.series", "insights.cards", "traffic.horizon"],
  traffic: [
    "traffic.series",
    "traffic.pages",
    "traffic.referrers",
    "traffic.journeys",
    "traffic.variants",
    "traffic.delivery",
    "traffic.horizon",
  ],
  search: ["search.queries", "search.pages", "search.trending"],
  assistant: ["assistant.summary"],
  feedback: ["feedback.list", "feedback.summary", "feedback.ratings", "feedback.pages"],
  truth: ["drift.open"],
  proposals: ["proposals.list"],
  deployments: ["builds.queue", "deployments.history"],
  automations: ["jobs.list"],
  content: ["content.tree"],
  settings: ["settings.integrations"],
};

export function renderPage(state: RouteState, data: PageData): Fragment {
  const renderer = RENDERERS[state.page];
  if (!renderer) return renderProblem("Unknown page", { status: 404, title: state.page });
  return renderer(state, data);
}

/** Endpoints a page needs that nothing serves, for the banner it shows. */
export function unservedFor(page: string): string[] {
  return (PAGE_ENDPOINTS[page] ?? []).filter((id) => findEndpoint(id)?.servedBy === "unbuilt");
}
