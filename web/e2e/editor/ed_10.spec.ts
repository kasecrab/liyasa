// ED-10's acceptance test.
//
// > Given rename, move, duplicate, and delete of a page and a drag in the
// > navigation tree; when saved; then `liyasa.json` navigation matches, slugs
// > are unchanged, a redirect was added for the move, and `id` front matter is
// > preserved.
//
// **This spec has never run and is skipped.** Every one of these operations is
// a `ProjectChange` computed by `web/editor/src/pages.ts` and covered by
// `web/editor/test/pages.test.ts`, including the redirect, the unchanged slug
// and the preserved id. Applying one needs
// `/_liyasa/editor/drafts/{id}/files`, which nothing serves.

import { expect, test } from "@playwright/test";

test.skip(true, "waits on the editor's draft file route");

async function config(request: import("@playwright/test").APIRequestContext) {
  return (await request.get("/_liyasa/editor/drafts/current/liyasa.json")).json();
}

test("a rename changes the title and not the URL", async ({ page, request }) => {
  await page.goto("/_liyasa/editor/?page=guides/limits.md&draft=new");
  await page.locator("[data-page-menu='guides/limits.md']").click();
  await page.locator("[data-action='rename']").click();
  await page.locator("[data-rename-title]").fill("Rate limits");
  await page.locator("[data-confirm]").click();

  expect(JSON.stringify(await config(request))).toContain("guides/limits.md");
  const file = await (await request.get("/_liyasa/editor/fs/guides/limits.md")).text();
  expect(file).toContain("title: Rate limits");
  expect(file).toMatch(/id: [0-9A-HJKMNP-TV-Z]{26}/);
});

test("a move adds a redirect from the old route and keeps the id", async ({ page, request }) => {
  const before = await (await request.get("/_liyasa/editor/fs/guides/limits.md")).text();
  const id = /id: (\S+)/.exec(before)?.[1];

  await page.goto("/_liyasa/editor/?page=guides/limits.md&draft=new");
  await page.locator("[data-page-menu='guides/limits.md']").click();
  await page.locator("[data-action='move']").click();
  await page.locator("[data-move-to]").fill("reference/limits.md");
  await page.locator("[data-confirm]").click();

  const after = await config(request);
  expect(after.redirects).toContainEqual({
    source: "/guides/limits",
    destination: "/reference/limits",
    status: 301,
  });
  const moved = await (await request.get("/_liyasa/editor/fs/reference/limits.md")).text();
  expect(moved).toContain(`id: ${id}`);
});

test("a duplicate gets its own id", async ({ page, request }) => {
  // Page identity is a ULID; two pages sharing one are the same page to the
  // truth graph, so an edge from either lands on both.
  const original = await (await request.get("/_liyasa/editor/fs/guides/limits.md")).text();
  await page.goto("/_liyasa/editor/?page=guides/limits.md&draft=new");
  await page.locator("[data-page-menu='guides/limits.md']").click();
  await page.locator("[data-action='duplicate']").click();
  await page.locator("[data-confirm]").click();
  const copy = await (await request.get("/_liyasa/editor/fs/guides/limits-copy.md")).text();
  expect(/id: (\S+)/.exec(copy)?.[1]).not.toEqual(/id: (\S+)/.exec(original)?.[1]);
});

test("a delete removes the page from the navigation as well as from disk", async ({ page, request }) => {
  await page.goto("/_liyasa/editor/?page=guides/hosting.md&draft=new");
  await page.locator("[data-page-menu='guides/hosting.md']").click();
  await page.locator("[data-action='delete']").click();
  await page.locator("[data-confirm]").click();
  expect(JSON.stringify(await config(request))).not.toContain("guides/hosting.md");
});

test("a drag in the tree writes the new order back to liyasa.json", async ({ page, request }) => {
  await page.goto("/_liyasa/editor/?page=guides/limits.md&draft=new");
  await page
    .locator("[data-nav-item='guides/hosting.md']")
    .dragTo(page.locator("[data-nav-group='Getting started']"));
  const tree = JSON.stringify((await config(request)).navigation);
  expect(tree.indexOf("guides/hosting.md")).toBeLessThan(tree.indexOf("guides/limits.md"));
});
