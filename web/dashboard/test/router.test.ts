// The hash router (ANA-70, ANA-71).

import test from "node:test";
import assert from "node:assert/strict";

import { DEFAULT_PAGE, isPage, PAGES, pageById, parseRoute, routeHref, routeRange } from "../src/router.ts";

const DAY = 86_400_000;
const T0 = 1_789_344_000_000;

test("an empty hash is the overview", () => {
  assert.equal(parseRoute("").page, DEFAULT_PAGE);
  assert.equal(parseRoute("#/").page, DEFAULT_PAGE);
});

test("a page that does not exist falls back rather than rendering nothing", () => {
  assert.equal(parseRoute("#/nonsense").page, DEFAULT_PAGE);
  assert.ok(!isPage("nonsense"));
  assert.equal(pageById("nonsense"), undefined);
  for (const page of PAGES) assert.ok(isPage(page.id));
});

test("a hash carries the range, the comparison, the grain and the filters", () => {
  const route = parseRoute("#/traffic?days=7&compare=1&grain=hour&version=v2&caller=agent&route=%2Fguides");
  assert.equal(route.page, "traffic");
  assert.deepEqual(route.rangeSpec, { kind: "last", days: 7 });
  assert.equal(route.compare, true);
  assert.equal(route.grain, "hour");
  assert.deepEqual(route.filters, { version: "v2", caller: "agent", routePrefix: "/guides" });
});

test("a pinned range survives the round trip", () => {
  const href = `#/traffic?from=${T0}&to=${T0 + DAY}`;
  const route = parseRoute(href);
  assert.deepEqual(route.rangeSpec, { kind: "between", from: T0, to: T0 + DAY });
  assert.deepEqual(routeRange(route, T0 + 90 * DAY), { from: T0, to: T0 + DAY });
  assert.equal(parseRoute(routeHref(route)).rangeSpec.kind, "between");
});

test("a nonsense range is the default rather than an empty window", () => {
  assert.deepEqual(parseRoute("#/traffic?days=nope").rangeSpec, { kind: "last", days: 28 });
  assert.deepEqual(parseRoute("#/traffic?from=5&to=1").rangeSpec, { kind: "last", days: 28 });
  assert.deepEqual(parseRoute("#/traffic?days=-4").rangeSpec, { kind: "last", days: 28 });
});

test("a grain the dashboard does not draw is ignored", () => {
  assert.equal(parseRoute("#/traffic?grain=fortnight").grain, undefined);
});

test("every route round-trips through its own href", () => {
  for (const page of PAGES) {
    const route = parseRoute(`#/${page.id}?days=90&compare=1&locale=pt-BR&focus=%2Fa`);
    assert.deepEqual(parseRoute(routeHref(route)), route, page.id);
  }
});

test("changing page keeps the range and the filters", () => {
  const route = parseRoute("#/traffic?days=7&version=v2");
  const moved = routeHref({ ...route, page: "search" });
  const after = parseRoute(moved);
  assert.equal(after.page, "search");
  assert.deepEqual(after.rangeSpec, route.rangeSpec);
  assert.deepEqual(after.filters, route.filters);
});
