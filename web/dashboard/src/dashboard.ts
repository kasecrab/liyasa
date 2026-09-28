// The entry point: route, fetch, render, wire.
//
// Everything above this file is pure. This is the only module that touches the
// document, the network or storage, and it is deliberately the shortest one.

import { attachChartKeys } from "./chart.ts";
import type { ChartSpec } from "./chart.ts";
import { read } from "./api.ts";
import type { Result } from "./api.ts";
import { html } from "./escape.ts";
import type { Fragment } from "./escape.ts";
import { filterOptionsFrom, renderNav, renderToolbar } from "./controls.ts";
import type { FilterOptions } from "./controls.ts";
import { PAGE_ENDPOINTS, renderPage, renderProblem, unservedFor } from "./pages.ts";
import type { PageData } from "./pages.ts";
import { DEFAULT_PAGE, PAGES, pageById, parseRoute, routeHref, routeRange } from "./router.ts";
import type { RouteState } from "./router.ts";
import { grainFor } from "./ranges.ts";
import { loadViews, removeView, saveViews, upsertView } from "./views.ts";
import type { SavedView } from "./views.ts";

const CHART_SPECS = new Map<string, ChartSpec>();

/** The shell, which is everything except the page body. */
export function renderShell(
  state: RouteState,
  body: Fragment,
  views: SavedView[],
  options: FilterOptions,
  nowMs: number,
): Fragment {
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

/**
 * What the toolbar needs, on every page rather than only on Traffic.
 *
 * The ANA-71 filter bar is part of the shell, so the values it offers cannot
 * come from `PAGE_ENDPOINTS` — a page that does not draw a variant split still
 * has to offer the filters. One extra read, outside the per-page set.
 */
const SHELL_ENDPOINT = "traffic.variants";

async function loadShell(state: RouteState, nowMs: number): Promise<FilterOptions> {
  const range = routeRange(state, nowMs);
  const split = await read<Record<string, Array<{ name: string }>>>(SHELL_ENDPOINT, {
    range,
    filters: {},
  });
  return filterOptionsFrom(split.ok ? split.value : undefined, state.filters);
}

async function loadPage(state: RouteState, nowMs: number): Promise<PageData> {
  const range = routeRange(state, nowMs);
  const grain = state.grain ?? grainFor(range);
  // `focus` travels to the API as `page`: an exact route, distinct from
  // ANA-71's `route` prefix filter. An endpoint that does not read it ignores
  // it, which is why it can go on every request rather than a chosen few.
  const spec = {
    range,
    grain,
    filters: state.filters,
    compare: state.compare,
    extra: { page: state.focus },
  };
  const ids = PAGE_ENDPOINTS[state.page] ?? [];
  const results = await Promise.all(ids.map((id) => read<unknown>(id, spec)));
  const data: PageData = {};
  ids.forEach((id, index) => {
    data[id] = results[index] as Result<unknown>;
  });
  return data;
}

function currentHash(): string {
  return typeof location === "undefined" ? "" : location.hash;
}

async function draw(root: Element): Promise<void> {
  const state = parseRoute(currentHash());
  const nowMs = Date.now();
  const views = loadViews(typeof localStorage === "undefined" ? undefined : localStorage);
  let body: Fragment;
  let options: FilterOptions = {};
  try {
    const [data, shell] = await Promise.all([loadPage(state, nowMs), loadShell(state, nowMs)]);
    options = shell;
    body = renderPage(state, data);
  } catch (error) {
    body = renderProblem("This page could not be drawn", {
      status: 0,
      title: "Unexpected failure",
      detail: String(error),
    });
  }
  root.innerHTML = String(renderShell(state, body, views, options, nowMs));
  attachChartKeys(root, CHART_SPECS);
  wire(root, state, views);
}

function wire(root: Element, state: RouteState, views: SavedView[]): void {
  for (const button of Array.from(root.querySelectorAll("[data-save-view]"))) {
    button.addEventListener("click", () => {
      const name = prompt("Name this view");
      if (!name) return;
      const view: SavedView = {
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
export function startDashboard(root: Element): void {
  if (!location.hash) location.hash = routeHref({ page: DEFAULT_PAGE, rangeSpec: { kind: "last", days: 28 }, filters: {}, compare: false });
  void draw(root);
  addEventListener("hashchange", () => void draw(root));
}

const mount = typeof document === "undefined" ? null : document.getElementById("dashboard");
if (mount) startDashboard(mount);
