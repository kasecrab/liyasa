// ED-10: create, rename, move, duplicate and delete a page, and drag the
// navigation tree, as changes to the project rather than as clicks.

import test from "node:test";
import assert from "node:assert/strict";

import {
  createPage,
  deletePage,
  duplicatePage,
  movePage,
  navigationOf,
  renamePage,
  reorderNavigation,
  routeOf,
} from "../src/pages.ts";

function project() {
  return {
    config: {
      name: "Acme",
      navigation: [
        { group: "Getting started", pages: ["index.md", "getting-started/install.md"] },
        { group: "Guides", pages: ["guides/limits.md", "guides/hosting.md"] },
      ],
      redirects: [{ source: "/old", destination: "/guides/limits", status: 301 }],
    },
    pages: {
      "index.md": "---\nid: 01J0000000000000000000HOME\ntitle: Home\n---\n\nWelcome.\n",
      "getting-started/install.md": "---\nid: 01J000000000000000000INST\ntitle: Install\n---\n\nRun it.\n",
      "guides/limits.md": "---\nid: 01J00000000000000000LIMIT\ntitle: Limits\n---\n\nThe caps.\n",
      "guides/hosting.md": "---\nid: 01J0000000000000000000HOST\ntitle: Hosting\n---\n\nWhere.\n",
    },
  };
}

test("a route is derived the way liyasa_build::nav::normalize derives it", () => {
  // The two have to agree or a redirect the editor writes points at a route
  // the build does not serve.
  assert.equal(routeOf("index.md"), "/");
  assert.equal(routeOf("guides/index.md"), "/guides");
  assert.equal(routeOf("guides/limits.md"), "/guides/limits");
  assert.equal(routeOf("/guides/limits.mdx"), "/guides/limits");
  assert.equal(routeOf("guides/"), "/guides");
});

test("creating a page writes the file and adds it to the navigation", () => {
  const change = createPage(project(), { path: "guides/quotas.md", title: "Quotas", group: "Guides" });
  assert.deepEqual(change.writes.map((write) => write.path), ["guides/quotas.md"]);
  assert.match(change.writes[0]?.text ?? "", /^---\nid: [0-9A-HJKMNP-TV-Z]{26}\ntitle: Quotas\n---\n\n/);
  assert.deepEqual(navigationOf(change.config)[1]?.pages, [
    "guides/limits.md",
    "guides/hosting.md",
    "guides/quotas.md",
  ]);
  assert.deepEqual(change.redirects, []);
});

test("creating a page into a group that does not exist is refused", () => {
  const change = createPage(project(), { path: "x.md", title: "X", group: "Nowhere" });
  assert.equal(change.diagnostics[0]?.code, "E0104");
  assert.deepEqual(change.writes, [], "nothing is written when the placement is wrong");
});

test("creating a page over an existing one is refused", () => {
  const change = createPage(project(), { path: "guides/limits.md", title: "Again", group: "Guides" });
  assert.equal(change.diagnostics[0]?.code, "E0105");
  assert.deepEqual(change.writes, []);
});

test("renaming changes the title and leaves the slug alone", () => {
  // The requirement in one line: a title change does not change a URL.
  const change = renamePage(project(), { path: "guides/limits.md", title: "Rate limits" });
  assert.deepEqual(change.moves, []);
  assert.deepEqual(change.redirects, []);
  assert.match(change.writes[0]?.text ?? "", /title: Rate limits/);
  assert.match(change.writes[0]?.text ?? "", /id: 01J00000000000000000LIMIT/);
  assert.deepEqual(navigationOf(change.config)[1]?.pages, ["guides/limits.md", "guides/hosting.md"]);
});

test("moving a page writes a redirect from the old route and keeps the id", () => {
  const change = movePage(project(), { from: "guides/limits.md", to: "reference/limits.md" });
  assert.deepEqual(change.moves, [{ from: "guides/limits.md", to: "reference/limits.md" }]);
  assert.deepEqual(change.redirects, [{ source: "/guides/limits", destination: "/reference/limits", status: 301 }]);
  assert.match(change.writes[0]?.text ?? "", /id: 01J00000000000000000LIMIT/);
  assert.deepEqual(navigationOf(change.config)[1]?.pages, ["reference/limits.md", "guides/hosting.md"]);
});

test("a move whose redirect collides with an existing rule is refused", () => {
  // Two rules with the same source is E0106 at build time. Writing one and
  // finding out at the next build is the editor handing the author a project
  // that does not build.
  const start = project();
  start.config.redirects = [{ source: "/guides/limits", destination: "/elsewhere", status: 301 }];
  const change = movePage(start, { from: "guides/limits.md", to: "reference/limits.md" });
  assert.equal(change.diagnostics[0]?.code, "E0106");
  assert.deepEqual(change.moves, []);
});

test("a move that would land on an existing page is refused", () => {
  const change = movePage(project(), { from: "guides/limits.md", to: "guides/hosting.md" });
  assert.equal(change.diagnostics[0]?.code, "E0105");
  assert.deepEqual(change.moves, []);
});

test("duplicating a page gives the copy a new id", () => {
  // A ULID is page identity: two pages sharing one are the same page to the
  // truth graph, so an edge from either lands on both.
  const change = duplicatePage(project(), { from: "guides/limits.md", to: "guides/limits-copy.md" });
  const text = change.writes[0]?.text ?? "";
  assert.doesNotMatch(text, /01J00000000000000000LIMIT/);
  assert.match(text, /^---\nid: [0-9A-HJKMNP-TV-Z]{26}\n/);
  assert.match(text, /title: Limits \(copy\)/);
  assert.deepEqual(change.redirects, [], "a copy is not a move");
});

test("deleting a page removes it from the navigation as well as from disk", () => {
  // A navigation entry pointing at a page that is gone is E0104 on the next
  // build.
  const change = deletePage(project(), { path: "guides/hosting.md" });
  assert.deepEqual(change.deletes, ["guides/hosting.md"]);
  assert.deepEqual(navigationOf(change.config)[1]?.pages, ["guides/limits.md"]);
});

test("a drag writes the new order back to liyasa.json", () => {
  const change = reorderNavigation(project(), {
    page: "guides/hosting.md",
    toGroup: "Getting started",
    toIndex: 1,
  });
  assert.deepEqual(navigationOf(change.config)[0]?.pages, [
    "index.md",
    "guides/hosting.md",
    "getting-started/install.md",
  ]);
  assert.deepEqual(navigationOf(change.config)[1]?.pages, ["guides/limits.md"]);
  // The file did not move, so no redirect and no rewrite.
  assert.deepEqual(change.moves, []);
  assert.deepEqual(change.redirects, []);
  assert.deepEqual(change.writes, []);
});

test("a drag leaves every other key of liyasa.json untouched", () => {
  const before = project();
  const change = reorderNavigation(before, { page: "guides/hosting.md", toGroup: "Guides", toIndex: 0 });
  const after = change.config as typeof before.config;
  assert.equal(after.name, "Acme");
  assert.deepEqual(after.redirects, before.config.redirects);
  assert.notEqual(after, before.config, "the change is a new object, not a mutation");
  assert.deepEqual(before.config.navigation[1]?.pages, ["guides/limits.md", "guides/hosting.md"]);
});

test("a navigation written as an object under `pages` is read and written in place", () => {
  // `navigation` is a tree, a file path, or an object with the tree under
  // `pages` or `tabs`. An editor that only understands the array form silently
  // drops `breadcrumbs`, `autofill` and `drilldown` on the first drag.
  const start = {
    config: {
      navigation: { breadcrumbs: "path", pages: [{ group: "G", pages: ["a.md", "b.md"] }] },
    },
    pages: { "a.md": "---\ntitle: A\n---\n", "b.md": "---\ntitle: B\n---\n" },
  };
  const change = reorderNavigation(start, { page: "b.md", toGroup: "G", toIndex: 0 });
  const after = change.config as { navigation: { breadcrumbs: string; pages: unknown[] } };
  assert.equal(after.navigation.breadcrumbs, "path");
  assert.deepEqual(navigationOf(change.config)[0]?.pages, ["b.md", "a.md"]);
});

test("a navigation held in a separate file is refused rather than half-written", () => {
  const change = reorderNavigation(
    { config: { navigation: "navigation.json" }, pages: {} },
    { page: "a.md", toGroup: "G", toIndex: 0 },
  );
  assert.equal(change.diagnostics[0]?.code, "E0104");
  assert.match(change.diagnostics[0]?.message ?? "", /navigation\.json/);
});
