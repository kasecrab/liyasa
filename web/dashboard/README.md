# `web/dashboard`

The dashboard of ANA-70: a TypeScript application the server serves, with the
eleven pages the requirement names — Overview, Traffic, Search, Assistant,
Feedback, Truth, Proposals, Deployments, Automations, Content, Settings — and
the range, comparison, filter and saved-view controls of ANA-71.

## What is here

| Module | What it is | Requirement |
|---|---|---|
| `src/escape.ts` | `html` and `Fragment`: the only escaper | — |
| `src/format.ts` | numbers, percentages, dates | — |
| `src/filters.ts` | the six filter dimensions and their query string | ANA-71 |
| `src/ranges.ts` | ranges, grains, period over period | ANA-71 |
| `src/views.ts` | saved views and the hash they open | ANA-71 |
| `src/router.ts` | the eleven pages and the hash router | ANA-70 |
| `src/chart.ts` | the accessible chart | ANA-70 |
| `src/controls.ts` | the toolbar | ANA-71 |
| `src/pages.ts` | one pure renderer per page | ANA-70 |
| `src/api.ts` | the HTTP calls, and which of them anything serves | ANA-70 |
| `src/dashboard.ts` | the only module that touches the document | — |

## Three decisions worth knowing before changing anything

**Rendering is pure.** Every renderer is a function from state and fetched data
to a `Fragment`, and `dashboard.ts` is the only module that touches the
document, the network or storage. That is why `test/` needs no browser and no
stand-in for one: a test against a fake DOM proves the fake DOM works.

**`html` returns a `Fragment`, not a string.** A fragment nests inside another
`html` untouched; a string is escaped. So markup composes, data cannot be
mistaken for markup, and there is no call anyone can forget.

**Eighteen endpoints have handlers, two never can, and none is reachable yet.**
`crates/liyasa-analytics/src/serve.rs` holds the handlers behind a default-off
`server` feature, driven as real HTTP in that crate's `tests/it/serve.rs`.
`liyasa-server`'s `Subtree` table is where they mount, and as of 2026-09-28 the
entry is not written: `AppState` has no handle on the analytics pool, so the
adapter has nothing to pass. That is one field and eight lines in WP-14's crate,
and `state/wp-17/PARKED` names it.

`drift.open` reads the verification store and `proposals.list` the editor's, so
neither can come from the analytics crate however the router is wired; both stay
`unbuilt` in `src/api.ts` and their pages say so.

A page whose data is unserved renders its controls and says so, rather than
drawing an empty chart — an empty chart reads as "no traffic", which is a
different and wrong statement. `read()` refuses an unbuilt endpoint without
making a request, because a 404 from a path nobody wired reads like an outage.

`servedBy` and `auth` on each endpoint are what make that legible to the seam as
well as to the page: `auth` is why ANA-02's published schema mounts as a second
entry with no permission, since a collector validates its events against that
document before it is allowed to post any.

**A page must draw what it fetches.** `test/wiring.test.ts` holds both
directions, and it exists because three endpoints were requested on every page
load and read by no renderer — the request succeeded, the page looked complete,
and the requirement clause behind it was unmet. `test/css.test.ts` holds the
same idea for styling: a class the renderers emit with no rule in
`dashboard.css` is markup the design system does not reach.

## Two contracts with the Rust side

Both are JSON files in `test/`, read by both languages. Neither side is checked
against the other's output, which is what makes them contracts rather than two
copies of one implementation agreeing with itself.

| Fixture | Rust side | What it protects |
|---|---|---|
| `test/filters.fixture.json` | `crates/liyasa-analytics/tests/it/query.rs` | a filter name that differs between the two is a filter that silently does nothing |
| `test/endpoints.fixture.json` | `crates/liyasa-analytics/tests/it/api.rs` | a path that exists on one side only is a 404 nobody sees until the page is open |

## Charts

Every chart carries its numbers as a `<table>`, always, inside a `<details>`
whose `<summary>` is ANA-70's toggle. It is not a fallback: a screen reader
reaches the numbers without toggling anything, and the table is built from the
same series the line is drawn from, so the two cannot disagree.

Keyboard navigation moves a cursor rather than tabbing through points. A year
of daily traffic is 365 points, and putting each one in the tab order makes the
chart a wall for everyone who navigates that way. The figure takes focus once,
arrow keys and Home and End move the cursor, and an `aria-live` region says
what is under it — in the same words the table row carries.

## The design system

Every colour, space and radius is a `--ly-*` token from `crates/liyasa-theme`.
The block at the top of `dashboard.css` is a fallback set — what the tokens
resolve to when the dashboard is opened without the theme stylesheet — and is
the only place in the file a literal colour appears. `test/css.test.ts` holds
that line, so light and dark stay the theme's decision.

## Building and testing

Run everything from `web/`, where the one `node_modules` lives:

```sh
cd ..
npm run build:dashboard   # writes dashboard/dist/dashboard.js
npm run test:dashboard    # the unit suite, under node --test
```

Neither needs anything installed. `build.mjs` reuses `bundle` from
`web/reader/build.mjs`, which WP-11 wrote against what Node 24 already has, for
the same reason: PRD §6.2 names Vite 8 for `web/` and `npm install` needs a
registry this machine does not always have. Both bundles therefore obey one set
of rules — relative `.ts` imports, no cycles, every binding declared once across
the graph, nothing a default export. `plan/rfcs/1100-reader-toolchain.md` is
where that trade is recorded.

`dist/` is committed for the same reason the reader's is: it is what a server
serves and what `test/build.test.ts` compares against, so a stale bundle is a
red test rather than a page that quietly runs last week's code.
