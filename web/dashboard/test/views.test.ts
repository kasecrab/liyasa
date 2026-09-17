// Saved views (ANA-71), and the hash they open.

import test from "node:test";
import assert from "node:assert/strict";

import {
  loadViews,
  removeView,
  resolveView,
  saveViews,
  upsertView,
  viewFromHref,
  viewHref,
  VIEWS_KEY,
} from "../src/views.ts";
import type { SavedView, ViewStore } from "../src/views.ts";
import { parseRoute, routeHref } from "../src/router.ts";

const DAY = 86_400_000;
const T0 = 1_789_344_000_000;

function store(initial: Record<string, string> = {}): ViewStore & { data: Record<string, string> } {
  const data = { ...initial };
  return {
    data,
    getItem: (key) => data[key] ?? null,
    setItem: (key, value) => {
      data[key] = value;
    },
  };
}

const view: SavedView = {
  id: "v1",
  name: "Payments, agents only",
  page: "traffic",
  range: { kind: "last", days: 28 },
  filters: { product: "payments", caller: "agent" },
  compare: true,
};

test("a relative view follows the calendar", () => {
  assert.notDeepEqual(resolveView(view, T0), resolveView(view, T0 + 30 * DAY));
  const pinned: SavedView = { ...view, range: { kind: "between", from: T0, to: T0 + DAY } };
  assert.deepEqual(resolveView(pinned, T0), resolveView(pinned, T0 + 30 * DAY));
});

test("a view round-trips through the hash it opens", () => {
  const href = viewHref(view);
  assert.match(href, /^#\/traffic\?/);
  const back = viewFromHref(href);
  assert.equal(back.page, "traffic");
  assert.deepEqual(back.range, view.range);
  assert.deepEqual(back.filters, view.filters);
  assert.equal(back.compare, true);
});

test("the hash a view opens is a route the router understands", () => {
  const route = parseRoute(viewHref(view));
  assert.equal(route.page, "traffic");
  assert.deepEqual(route.filters, view.filters);
  assert.equal(route.compare, true);
  assert.deepEqual(route.rangeSpec, { kind: "last", days: 28 });
  assert.equal(routeHref(route), viewHref(view), "and the two agree on the spelling");
});

test("views are stored and read back", () => {
  const backing = store();
  saveViews(backing, [view]);
  assert.deepEqual(loadViews(backing), [view]);
  assert.ok(backing.data[VIEWS_KEY]);
});

test("a store that throws loses the view and not the page", () => {
  const broken: ViewStore = {
    getItem() {
      throw new Error("private mode");
    },
    setItem() {
      throw new Error("private mode");
    },
  };
  assert.deepEqual(loadViews(broken), []);
  assert.doesNotThrow(() => saveViews(broken, [view]));
  assert.deepEqual(loadViews(undefined), []);
});

test("rubbish in storage is no views rather than a crash", () => {
  assert.deepEqual(loadViews(store({ [VIEWS_KEY]: "not json" })), []);
  assert.deepEqual(loadViews(store({ [VIEWS_KEY]: '{"not":"an array"}' })), []);
  assert.deepEqual(loadViews(store({ [VIEWS_KEY]: '[{"nope":1}]' })), []);
});

test("saving the same name twice replaces rather than duplicates", () => {
  const renamed: SavedView = { ...view, id: "v2", filters: { caller: "human" } };
  const after = upsertView([view], renamed);
  assert.equal(after.length, 1);
  assert.deepEqual(after[0]!.filters, { caller: "human" });
  // The same name on another page is a different view.
  const elsewhere: SavedView = { ...view, id: "v3", page: "search" };
  assert.equal(upsertView(after, elsewhere).length, 2);
});

test("removing a view removes exactly one", () => {
  const two = upsertView([view], { ...view, id: "v2", name: "Other" });
  assert.equal(removeView(two, "v1").length, 1);
  assert.equal(removeView(two, "nope").length, 2);
});

test("a link with no range falls back to a sensible one rather than to nothing", () => {
  const back = viewFromHref("#/search");
  assert.deepEqual(back.range, { kind: "last", days: 28 });
  assert.equal(back.page, "search");
});
