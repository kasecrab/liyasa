// The ANA-71 toolbar: the date range, the comparison switch, the filter bar and
// the saved views.
//
// The filter bar is the reason this file exists. It was rendered on every page
// with an empty options object, so it offered nothing: the control was present,
// keyboard-reachable and completely inert. Nothing else in the suite could see
// that, because every assertion was about a control that HAD values.

import test from "node:test";
import assert from "node:assert/strict";

import {
  filterOptionsFrom,
  renderCompareToggle,
  renderFilterBar,
  renderNav,
  renderRangePicker,
  renderSavedViews,
  renderToolbar,
} from "../src/controls.ts";
import { CALLER_KINDS } from "../src/filters.ts";
import { lastDays } from "../src/ranges.ts";
import { PAGES, parseRoute } from "../src/router.ts";
import type { SavedView } from "../src/views.ts";

const T0 = 1_789_344_000_000;

function state(hash = "#/traffic?days=28") {
  return parseRoute(hash);
}

const SPLIT = {
  version: [{ name: "v2" }, { name: "v1" }],
  locale: [{ name: "en" }, { name: "de" }],
  region: [],
  product: [],
};

test("the filter bar offers the values the data actually has", () => {
  const options = filterOptionsFrom(SPLIT, {});
  assert.deepEqual(options.version, ["v2", "v1"]);
  assert.deepEqual(options.locale, ["en", "de"]);
  assert.deepEqual(options.region, [], "a site with no regions offers no region filter");
  assert.deepEqual(options.caller, CALLER_KINDS, "caller type is a closed set, always offered");
});

test("with no data the caller filter still works and the rest offer nothing", () => {
  const options = filterOptionsFrom(undefined, {});
  assert.deepEqual(options.caller, CALLER_KINDS);
  assert.deepEqual(options.version, []);
});

test("a value already filtered to survives even when the split no longer lists it", () => {
  // Filtering to v1 narrows the data to v1, so a later split may not mention
  // it. Dropping the option would leave no way to clear the filter.
  const options = filterOptionsFrom({ version: [{ name: "v1" }] }, { version: "v9" });
  assert.deepEqual(options.version, ["v9", "v1"]);
});

test("the filter bar renders a pressable link per value", () => {
  const markup = String(renderFilterBar(state(), filterOptionsFrom(SPLIT, {})));
  assert.match(markup, /data-dimension="version"/);
  assert.match(markup, /data-dimension="caller"/);
  assert.match(markup, />v2<\/a>/);
  assert.match(markup, />agent<\/a>/);
  assert.match(markup, /aria-pressed="false"/);
  assert.ok(!markup.includes('data-dimension="region"'), "an empty dimension is not a heading");
});

test("an inert filter bar is the bug this file was written for", () => {
  // The state the dashboard shipped in: options empty, nothing filtered, so
  // every dimension skipped and the control offered nothing at all.
  const inert = String(renderFilterBar(state(), {}));
  assert.ok(!inert.includes("data-dimension"), "this is what the defect looked like");
  const working = String(renderFilterBar(state(), filterOptionsFrom(SPLIT, {})));
  assert.notEqual(inert, working);
});

test("a chosen value reads as pressed and its link clears it", () => {
  const chosen = state("#/traffic?days=28&version=v2");
  const markup = String(renderFilterBar(chosen, filterOptionsFrom(SPLIT, chosen.filters)));
  assert.match(markup, /aria-pressed="true"/);
  assert.match(markup, /Clear 1/);
});

test("the range picker marks the current preset and names the window", () => {
  const markup = String(renderRangePicker(state("#/traffic?days=7"), lastDays(T0, 7)));
  assert.match(markup, /aria-current="true"/);
  assert.match(markup, /Last 7 days/);
  assert.match(markup, /href="#\/traffic\?days=28/, "the other presets are still offered");
});

test("the comparison control is a switch that reports its state", () => {
  assert.match(String(renderCompareToggle(state())), /role="switch"[^>]*aria-checked="false"/);
  assert.match(
    String(renderCompareToggle(state("#/traffic?days=28&compare=1"))),
    /aria-checked="true"/,
  );
});

test("saved views list only this page's and offer to save the current one", () => {
  const views: SavedView[] = [
    { id: "v1", name: "Agents only", page: "traffic", range: { kind: "last", days: 28 }, filters: { caller: "agent" }, compare: false },
    { id: "v2", name: "Elsewhere", page: "search", range: { kind: "last", days: 28 }, filters: {}, compare: false },
  ];
  const markup = String(renderSavedViews(state(), views));
  assert.match(markup, /Agents only/);
  assert.ok(!markup.includes("Elsewhere"), "another page's view is not this page's");
  assert.match(markup, /data-save-view="traffic"/);
  assert.match(markup, /data-remove-view="v1"/);
  assert.match(String(renderSavedViews(state(), [])), /none yet/);
});

test("the toolbar carries all four controls", () => {
  const markup = String(
    renderToolbar(state(), lastDays(T0, 28), filterOptionsFrom(SPLIT, {}), []),
  );
  for (const marker of ["ly-range", "ly-compare", "ly-filters", "ly-views"]) {
    assert.match(markup, new RegExp(marker), marker);
  }
});

test("the navigation marks the current page and links the rest", () => {
  const markup = String(renderNav(state("#/search?days=28"), PAGES));
  assert.match(markup, /aria-current="page"/);
  assert.equal((markup.match(/aria-current="page"/g) ?? []).length, 1);
  assert.equal((markup.match(/<li>/g) ?? []).length, PAGES.length);
});

test("a filter value that looks like markup does not become markup", () => {
  const markup = String(
    renderFilterBar(state(), { version: ['"><script>alert(1)</script>'], caller: [] }),
  );
  assert.ok(!markup.includes("<script>"));
  assert.match(markup, /&lt;script&gt;/);
});
