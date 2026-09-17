// The eleven pages of ANA-70, and the hash router over them.
//
// A hash rather than the history API: the dashboard is served from one path by
// a server whose fallback route serves the site, so a deep link under a real
// path would have to be routed there too — a change in another package for no
// gain to anybody.

import { filtersFromQuery } from "./filters.ts";
import type { Filters } from "./filters.ts";
import type { Grain, Range, RangeSpec } from "./ranges.ts";
import { resolveRange } from "./ranges.ts";

export interface PageId {
  id: string;
  label: string;
  /** What the page is for, shown under the heading. */
  summary: string;
}

/** ANA-70's list, in the order the requirement gives them. */
export const PAGES: PageId[] = [
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

export const DEFAULT_PAGE = "overview";

export function isPage(id: string): boolean {
  return PAGES.some((page) => page.id === id);
}

export function pageById(id: string): PageId | undefined {
  return PAGES.find((page) => page.id === id);
}

/** Everything a hash carries. */
export interface RouteState {
  page: string;
  rangeSpec: RangeSpec;
  filters: Filters;
  compare: boolean;
  grain?: Grain;
  /** A page-scoped selection, such as the route a Feedback page is filtered to. */
  focus?: string;
}

export function parseRoute(hash: string): RouteState {
  const [path = "", query = ""] = hash.replace(/^#\/?/, "").split("?");
  const page = isPage(path) ? path : DEFAULT_PAGE;
  const parameters = new URLSearchParams(query);
  const days = Number(parameters.get("days"));
  const from = Number(parameters.get("from"));
  const to = Number(parameters.get("to"));
  let rangeSpec: RangeSpec = { kind: "last", days: 28 };
  if (Number.isFinite(days) && days > 0) rangeSpec = { kind: "last", days };
  else if (Number.isFinite(from) && Number.isFinite(to) && to > from) {
    rangeSpec = { kind: "between", from, to };
  }
  const grain = parameters.get("grain");
  const focus = parameters.get("focus");
  const state: RouteState = {
    page,
    rangeSpec,
    filters: filtersFromQuery(query),
    compare: parameters.get("compare") === "1",
  };
  if (grain === "hour" || grain === "day") state.grain = grain;
  if (focus) state.focus = focus;
  return state;
}

export function routeRange(state: RouteState, now: number): Range {
  return resolveRange(state.rangeSpec, now);
}

/** The inverse, so every control on the page can produce a link. */
export function routeHref(state: RouteState): string {
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
