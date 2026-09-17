// ED-50's acceptance test.
//
// > Given human drafts and agent proposals; when the review queue is filtered
// > by source and page; then rows show summary, evidence, verification status,
// > and preview link.
//
// **This spec has never run and is skipped.** The queue's filtering and the
// columns a row carries are covered today by `web/editor/test/review.test.ts`.
// The queue itself needs `/_liyasa/editor/queue`, which nothing serves, and
// `web/editor/src/api.ts` marks it `unbuilt` so the pane says so rather than
// drawing an empty list.
//
// It lives under `web/e2e/dashboard/` because that is where the packet's
// acceptance table puts it: the unified queue is a dashboard surface, and it
// lists proposals from the editor and from automation side by side.

import { expect, test } from "@playwright/test";

test.skip(true, "waits on the editor's queue route");

test("the queue lists human drafts and agent proposals together", async ({ page }) => {
  await page.goto("/_liyasa/dashboard/#/proposals");
  await expect(page.locator("[data-queue-row]")).not.toHaveCount(0);
  await expect(page.locator("[data-source='human']")).not.toHaveCount(0);
  await expect(page.locator("[data-source='agent']")).not.toHaveCount(0);
});

test("a row carries every column the requirement names", async ({ page }) => {
  await page.goto("/_liyasa/dashboard/#/proposals");
  const row = page.locator("[data-queue-row]").first();
  for (const column of ["summary", "evidence", "verification", "preview", "pages", "age"]) {
    await expect(row.locator(`[data-column='${column}']`)).toBeVisible();
  }
});

test("filtering by source and by page narrows the list", async ({ page }) => {
  await page.goto("/_liyasa/dashboard/#/proposals");
  const all = await page.locator("[data-queue-row]").count();
  await page.locator("[data-filter-source='agent']").click();
  const agents = await page.locator("[data-queue-row]").count();
  expect(agents).toBeLessThan(all);

  await page.locator("[data-filter-page]").fill("guides/limits.md");
  await expect(page.locator("[data-queue-row]")).not.toHaveCount(agents);
});

test("a proposal with no preview says so rather than linking nowhere", async ({ page }) => {
  // A link to a 404 reads as a broken preview; the reviewer concludes the
  // build failed rather than that no preview was asked for.
  await page.goto("/_liyasa/dashboard/#/proposals");
  const without = page.locator("[data-queue-row][data-preview='none']").first();
  await expect(without.locator("[data-column='preview']")).toHaveText("No preview built");
  await expect(without.locator("[data-column='preview'] a")).toHaveCount(0);
});

test("filtering by reviewer shows what is waiting on me", async ({ page }) => {
  await page.goto("/_liyasa/dashboard/#/proposals");
  await page.locator("[data-filter-reviewer='me']").click();
  await expect(page.locator("[data-queue-row]")).not.toHaveCount(0);
});
