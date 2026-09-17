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

/** A series payload as a chart, with ANA-10's label when it is client measured. */
export function seriesChart(
  id: string,
  title: string,
  payload: SeriesPayload,
  delivery?: number | null,
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
  if (payload.sampledByClient) spec.note = `Sampled by client${measured}`;
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

export function renderTable(headers: string[], body: string[][]): Fragment {
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

export function renderTraffic(state: RouteState, data: PageData): Fragment {
  const series = data["traffic.series"] as Result<SeriesPayload> | undefined;
  const pages = data["traffic.pages"] as Result<{ pages: PageRow[] }> | undefined;
  const referrers = data["traffic.referrers"] as Result<{ referrers: NameRow[] }> | undefined;
  const journeys = data["traffic.journeys"] as
    | Result<{ entry: NameRow[]; exit: NameRow[] }>
    | undefined;
  const delivery = data["traffic.delivery"] as Result<{ ratio: number | null }> | undefined;
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
    const gaps = renderTable(["Topic"], value.unanswered.map((topic) => [topic]));
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

export function renderFeedback(state: RouteState, data: PageData): Fragment {
  const list = data["feedback.list"] as Result<{ items: FeedbackRow[] }> | undefined;
  const summary = data["feedback.summary"] as Result<{ up: number; down: number }> | undefined;

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

export function renderPage(state: RouteState, data: PageData): Fragment {
  const renderer = RENDERERS[state.page];
  if (!renderer) return renderProblem("Unknown page", { status: 404, title: state.page });
  return renderer(state, data);
}

/** Endpoints a page needs that nothing serves, for the banner it shows. */
export function unservedFor(page: string): string[] {
  return (PAGE_ENDPOINTS[page] ?? []).filter((id) => findEndpoint(id)?.servedBy === "unbuilt");
}
