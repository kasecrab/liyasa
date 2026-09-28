// The eleven pages of ANA-70.
//
// Every renderer is a pure function to an HTML string, so these assert on the
// artifact itself. There is no stand-in for a browser here on purpose: a test
// against a fake DOM proves the fake DOM works.

import test from "node:test";
import assert from "node:assert/strict";

import {
  PAGE_ENDPOINTS,
  RENDERERS,
  ratingsChart,
  renderRatedPages,
  renderVariants,
  renderDeliveryNote,
  renderInsightList,
  renderPage,
  renderProblem,
  renderTable,
  seriesChart,
  unservedFor,
} from "../src/pages.ts";
import type { PageData, SeriesPayload } from "../src/pages.ts";
import { PAGES, parseRoute } from "../src/router.ts";
import { findEndpoint } from "../src/api.ts";

const T0 = 1_789_344_000_000;
const HOUR = 3_600_000;

function state(page: string) {
  return parseRoute(`#/${page}?days=28`);
}

test("ANA-70's eleven pages all exist and all render", () => {
  assert.equal(PAGES.length, 11);
  for (const page of PAGES) {
    assert.ok(RENDERERS[page.id], `${page.id} has no renderer`);
    assert.ok(PAGE_ENDPOINTS[page.id], `${page.id} reads nothing`);
    const markup = String(renderPage(state(page.id), {}));
    assert.ok(markup.length > 0, `${page.id} rendered nothing`);
  }
});

test("the page list is the one the requirement gives", () => {
  assert.deepEqual(
    PAGES.map((page) => page.id),
    [
      "overview",
      "traffic",
      "search",
      "assistant",
      "feedback",
      "truth",
      "proposals",
      "deployments",
      "automations",
      "content",
      "settings",
    ],
  );
});

test("a page with nothing loaded says so rather than showing zeroes", () => {
  const markup = String(renderPage(state("traffic"), {}));
  assert.match(markup, /Not loaded/);
  assert.ok(!markup.includes("<svg"), "an empty chart would read as no traffic");
});

test("an endpoint nobody serves is named rather than drawn as empty", () => {
  const data: PageData = {
    "traffic.series": {
      ok: false,
      problem: { status: 501, title: "Not served yet", detail: "ANA-10: no handler" },
    },
  };
  const markup = String(renderPage(state("traffic"), data));
  assert.match(markup, /data-unserved="true"/);
  assert.match(markup, /No data is being served for this yet/);
  assert.match(markup, /ANA-10: no handler/);
});

test("the pages that need unbuilt endpoints name them", () => {
  // Two endpoints are left: drift belongs to the verification store and
  // proposals to the editor's, so neither can come from the analytics crate
  // however the router is wired.
  assert.deepEqual(unservedFor("truth"), ["drift.open"]);
  assert.deepEqual(unservedFor("proposals"), ["proposals.list"]);
  for (const id of [...unservedFor("truth"), ...unservedFor("proposals")]) {
    assert.equal(findEndpoint(id)?.servedBy, "unbuilt");
  }
  assert.deepEqual(unservedFor("automations"), [], "jobs are served by WP-14 today");
  assert.deepEqual(
    unservedFor("traffic"),
    [],
    "every traffic endpoint is served by liyasa_analytics::serve::mount now",
  );
});

test("a series draws with the caller split ANA-10 asks for", () => {
  const payload: SeriesPayload = {
    grain: "hour",
    source: "rollup",
    sampledByClient: false,
    points: [
      { bucket: T0, human: 3, agent: 2, bot: 1, integration: 0 },
      { bucket: T0 + HOUR, human: 4, agent: 0, bot: 0, integration: 0 },
    ],
  };
  const spec = seriesChart("x", "Page views", payload);
  assert.deepEqual(
    spec.series.map((s) => s.key),
    ["human", "agent", "bot"],
  );
  assert.deepEqual(spec.series[0]!.values, [3, 4]);
  assert.equal(spec.note, undefined);
});

test("a client measured series carries the label and the measured ratio", () => {
  const payload: SeriesPayload = {
    grain: "day",
    source: "raw",
    sampledByClient: true,
    points: [{ bucket: T0, human: 1, agent: 0, bot: 0, integration: 0 }],
  };
  assert.match(seriesChart("x", "Scroll depth", payload, 0.71).note ?? "", /Sampled by client/);
  assert.match(seriesChart("x", "Scroll depth", payload, 0.71).note ?? "", /71%/);
  assert.equal(
    seriesChart("x", "Scroll depth", payload, null).note,
    "Sampled by client",
    "no ratio to report is not a ratio of zero",
  );
});

test("the delivery note reports a measurement and not an assumption", () => {
  assert.match(String(renderDeliveryNote(0.71)), /71% of page loads delivered a client beacon/);
  assert.match(String(renderDeliveryNote(null)), /no beacon delivery ratio to measure/);
  assert.equal(renderDeliveryNote(undefined), null, "nothing measured, nothing claimed");
});

test("an empty table says so rather than rendering a header over nothing", () => {
  assert.match(String(renderTable(["A"], [])), /Nothing over this period/);
  assert.match(String(renderTable(["A"], [["x"]])), /<td>x<\/td>/);
});

test("a route from a request does not become markup", () => {
  const markup = String(renderTable(["Page"], [['<img src=x onerror="alert(1)">']]));
  assert.ok(!markup.includes("<img"));
  assert.match(markup, /&lt;img/);
});

test("an insight card renders its action as a button the page can wire", () => {
  const markup = String(renderInsightList([
    {
      kind: "unanswered_demand",
      title: "`sso saml` finds nothing",
      detail: "6 searches returned nothing",
      metrics: {},
      action: { kind: "create_page", label: "Create a page for this", target: "sso saml" },
    },
  ]));
  assert.match(markup, /data-action="create_page"/);
  assert.match(markup, /data-target="sso saml"/);
  assert.match(markup, /Create a page for this/);
});

test("no insights is a sentence rather than an empty list", () => {
  assert.match(String(renderInsightList([])), /Nothing stood out/);
});

test("a problem renders without a detail it does not have", () => {
  const markup = String(renderProblem("Traffic", { status: 500, title: "Server error" }));
  assert.match(markup, /Server error/);
  assert.ok(!markup.includes("ly-problem-detail"));
});

test("the deployments page offers the trigger and the queue position", () => {
  const data: PageData = {
    "builds.queue": {
      ok: true,
      value: {
        items: [{ jobId: "01J", project: "acme", position: 2, estimatedStartMs: T0 }],
        depth: 3,
        running: 1,
      },
    },
  };
  const markup = String(renderPage(state("deployments"), data));
  assert.match(markup, /data-action="trigger_build"/, "GIT-21: deploy from the dashboard");
  assert.match(markup, /<td>2<\/td>/, "GIT-24: the dashboard shows queue position");
  assert.match(markup, /Waiting/);
});

test("the feedback page keeps agent reports out of the score", () => {
  const data: PageData = {
    "feedback.summary": { ok: true, value: { up: 3, down: 1 } },
    "feedback.list": {
      ok: true,
      value: {
        items: [
          { id: "f1", route: "/a", kind: "page", rating: -1, text: "wrong", status: "open" },
          { id: "f2", route: "/a", kind: "agent", task: "create a payment", status: "open" },
        ],
      },
    },
  };
  const markup = String(renderPage(state("feedback"), data));
  assert.match(markup, /75%/, "three up out of four votes");
  assert.match(markup, /From agents/);
  assert.match(markup, /create a payment/);
  assert.match(markup, /never inside it/);
});

test("the settings page names integrations that would never load", () => {
  const data: PageData = {
    "settings.integrations": {
      ok: true,
      value: {
        enabled: [{ key: "ga4", name: "Google Analytics 4", consent: "required", loadsBeforeConsent: false }],
        stuck: ["ga4"],
        consentStatement: "Liyasa's own analytics set no cookies.",
      },
    },
  };
  const markup = String(renderPage(state("settings"), data));
  assert.match(markup, /never load/);
  assert.match(markup, /Google Analytics 4/);
  assert.match(markup, /set no cookies/);
});

test("the variant split shows each dimension and links to its filter", () => {
  const markup = String(
    renderVariants(state("traffic"), {
      version: [
        { name: "v2", count: 30 },
        { name: "v1", count: 10 },
      ],
      locale: [{ name: "en", count: 40 }],
      region: [],
      product: [],
    }),
  );
  assert.match(markup, /data-dimension="version"/);
  assert.match(markup, /data-dimension="locale"/);
  assert.ok(!markup.includes('data-dimension="region"'), "an empty dimension is not a heading");
  assert.match(markup, /75%/, "v2 is 30 of 40 reads");
  assert.match(markup, /25%/);
  // The share is a link that sets the filter, so the answer is shareable.
  assert.match(markup, /href="#\/traffic\?[^"]*version=v2/);
});

test("a dimension already filtered to shows as pressed and clears on a second click", () => {
  const filtered = { ...state("traffic"), filters: { version: "v2" } };
  const markup = String(
    renderVariants(filtered, {
      version: [{ name: "v2", count: 30 }],
      locale: [],
      region: [],
      product: [],
    }),
  );
  assert.match(markup, /aria-pressed="true"/);
  assert.ok(
    !/href="#\/traffic\?[^"]*version=v2/.test(markup),
    "clicking the chosen one clears it rather than setting it again",
  );
});

test("nothing recorded for any dimension is a sentence rather than four empty lists", () => {
  const markup = String(
    renderVariants(state("traffic"), { version: [], locale: [], region: [], product: [] }),
  );
  assert.match(markup, /No version, locale, region or product was recorded/);
});

test("ratings over time chart both directions, not a single score line", () => {
  const spec = ratingsChart("r", {
    grain: "day",
    route: "/payments",
    points: [
      { bucket: T0, up: 3, down: 1 },
      { bucket: T0 + 86_400_000, up: 0, down: 4 },
    ],
  });
  assert.equal(spec.title, "Ratings for /payments");
  assert.equal(spec.kind, "bar");
  assert.deepEqual(
    spec.series.map((s) => s.key),
    ["up", "down"],
    "a score line would hide the denominator",
  );
  assert.deepEqual(spec.series[0]!.values, [3, 0]);
  assert.deepEqual(spec.series[1]!.values, [1, 4]);

  const wide = ratingsChart("r", { grain: "day", route: null, points: [] });
  assert.equal(wide.title, "Ratings, site-wide");
});

test("the per-page standing keeps agent reports out of the score", () => {
  const markup = String(
    renderRatedPages(state("feedback"), [
      { route: "/payments", up: 3, down: 1, agentReports: 2, open: 4 },
      { route: "/unrated", up: 0, down: 0, agentReports: 1, open: 1 },
    ]),
  );
  assert.match(markup, /<td>75%<\/td>/, "three of four votes, and the two agent reports are not votes");
  assert.match(markup, /<td>—<\/td>/, "a page with no votes has no score rather than 0%");
  assert.match(markup, /href="#\/feedback\?[^"]*focus=%2Fpayments/);
});

test("nothing rated is a sentence rather than an empty table", () => {
  assert.match(String(renderRatedPages(state("feedback"), [])), /Nothing rated/);
});

test("a page route that looks like markup does not become markup in the standing", () => {
  const markup = String(
    renderRatedPages(state("feedback"), [
      { route: '<img onerror="alert(1)">', up: 1, down: 0, agentReports: 0, open: 0 },
    ]),
  );
  assert.ok(!markup.includes("<img"));
  assert.match(markup, /&lt;img/);
});

test("the feedback page draws the ratings chart and the standing it fetches", () => {
  const data: PageData = {
    "feedback.summary": { ok: true, value: { up: 3, down: 1 } },
    "feedback.list": { ok: true, value: { items: [] } },
    "feedback.ratings": {
      ok: true,
      value: { grain: "day", route: null, points: [{ bucket: T0, up: 2, down: 1 }] },
    },
    "feedback.pages": {
      ok: true,
      value: { pages: [{ route: "/a", up: 2, down: 1, agentReports: 0, open: 1 }] },
    },
  };
  const markup = String(renderPage(state("feedback"), data));
  assert.match(markup, /Ratings over time/);
  assert.match(markup, /Ratings, site-wide/);
  assert.match(markup, /By page/);
  assert.match(markup, /<td><a href="[^"]*">\/a<\/a><\/td>/);
});

test("the traffic page draws the variant split it fetches", () => {
  const data: PageData = {
    "traffic.variants": {
      ok: true,
      value: { version: [{ name: "v2", count: 5 }], locale: [], region: [], product: [] },
    },
  };
  const markup = String(renderPage(state("traffic"), data));
  assert.match(markup, /Version, locale, region and product/);
  assert.match(markup, /data-dimension="version"/);
});

test("an unknown page is a problem rather than a blank screen", () => {
  assert.match(String(renderPage({ ...state("overview"), page: "nonsense" }, {})), /Unknown page/);
});
