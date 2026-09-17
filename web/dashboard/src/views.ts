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

import { filtersFromQuery, filtersToQuery } from "./filters.ts";
import type { Filters } from "./filters.ts";
import type { Grain, Range, RangeSpec } from "./ranges.ts";
import { resolveRange } from "./ranges.ts";

export const VIEWS_KEY = "liyasa.dashboard.views";

export interface SavedView {
  id: string;
  name: string;
  page: string;
  range: RangeSpec;
  filters: Filters;
  compare: boolean;
  grain?: Grain;
}

export interface ViewStore {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
}

export function resolveView(view: SavedView, now: number): Range {
  return resolveRange(view.range, now);
}

/** Reads the saved views, treating anything unreadable as none. */
export function loadViews(store: ViewStore | undefined): SavedView[] {
  if (!store) return [];
  let text: string | null = null;
  try {
    text = store.getItem(VIEWS_KEY);
  } catch {
    return [];
  }
  if (!text) return [];
  try {
    const parsed: unknown = JSON.parse(text);
    return Array.isArray(parsed) ? parsed.filter(isView) : [];
  } catch {
    return [];
  }
}

export function saveViews(store: ViewStore | undefined, views: SavedView[]): void {
  if (!store) return;
  try {
    store.setItem(VIEWS_KEY, JSON.stringify(views));
  } catch {
    // A full or disabled store loses the view, and losing a bookmark is not
    // worth failing the page over.
  }
}

/** Adds a view, replacing one of the same name on the same page. */
export function upsertView(views: SavedView[], view: SavedView): SavedView[] {
  const without = views.filter((v) => !(v.page === view.page && v.name === view.name));
  return [...without, view];
}

export function removeView(views: SavedView[], id: string): SavedView[] {
  return views.filter((view) => view.id !== id);
}

/** The URL fragment a view opens, which is also what a shared link carries. */
export function viewHref(view: SavedView): string {
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
export function viewFromHref(href: string, name = "Shared view"): SavedView {
  const [path, query = ""] = href.replace(/^#\/?/, "").split("?");
  const parameters = new URLSearchParams(query);
  const days = Number(parameters.get("days"));
  const from = Number(parameters.get("from"));
  const to = Number(parameters.get("to"));
  const range: RangeSpec =
    Number.isFinite(days) && days > 0
      ? { kind: "last", days }
      : Number.isFinite(from) && Number.isFinite(to) && to > 0
        ? { kind: "between", from, to }
        : { kind: "last", days: 28 };
  const grain = parameters.get("grain");
  const view: SavedView = {
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

function isView(value: unknown): value is SavedView {
  if (typeof value !== "object" || value === null) return false;
  const candidate = value as Record<string, unknown>;
  return (
    typeof candidate["id"] === "string" &&
    typeof candidate["name"] === "string" &&
    typeof candidate["page"] === "string" &&
    typeof candidate["range"] === "object" &&
    candidate["range"] !== null
  );
}
