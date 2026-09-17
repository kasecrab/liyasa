// ED-12's acceptance test.
//
// > Given a find-and-replace across 50 fixture pages including inside props;
// > when previewed and applied; then every occurrence is replaced and the diff
// > preview matched the result.
//
// **This spec has never run and is skipped.** The 50 pages, the scope rules
// and the identity of preview and result are covered today by
// `web/editor/test/bulk.test.ts`, against segmentations
// `tests/editor/segments.rs` produced with the real scanner. Applying the plan
// needs the draft file route, which nothing serves.

import { expect, test } from "@playwright/test";

test.skip(true, "waits on the editor's draft file route");

test("a replace across fifty pages previews and applies the same thing", async ({ page }) => {
  await page.goto("/_liyasa/editor/?draft=new#/find");
  await page.locator("[data-find]").fill("Widget API");
  await page.locator("[data-replace]").fill("Gadget API");
  await page.locator("[data-preview-replace]").click();

  await expect(page.locator("[data-match-count]")).toHaveText("200 matches on 50 pages");
  const previewed = await page.locator("[data-diff='guides/page-00.md'] ins").allTextContents();

  await page.locator("[data-apply-replace]").click();
  await expect(page.locator("[data-applied]")).toHaveText("50 pages changed");

  const applied = await page.locator("[data-result='guides/page-00.md']").textContent();
  for (const line of previewed) expect(applied).toContain(line);
});

test("a directive prop is replaced and a fenced command is not", async ({ page }) => {
  // Rewriting a command inside a fence silently changes a command a reader
  // will run, so `code` is opt-in and `props` is not.
  await page.goto("/_liyasa/editor/?draft=new#/find");
  await page.locator("[data-find]").fill("Widget");
  await page.locator("[data-replace]").fill("Gadget");
  await page.locator("[data-preview-replace]").click();
  await expect(page.locator("[data-diff='guides/page-00.md']")).toContainText('title="The Gadget API"');
  await expect(page.locator("[data-diff='guides/page-00.md'] code")).toContainText(
    "example.invalid/Widget/API",
  );
});

test("a template expression is never rewritten", async ({ page }) => {
  await page.goto("/_liyasa/editor/?draft=new#/find");
  await page.locator("[data-find]").fill("site.name");
  await page.locator("[data-replace]").fill("site.title");
  await page.locator("[data-preview-replace]").click();
  await expect(page.locator("[data-match-count]")).toHaveText("no matches");
});

test("a changed fact reference reaches every page that reads it", async ({ page }) => {
  await page.goto("/_liyasa/editor/?draft=new#/find");
  await page.locator("[data-find-fact]").fill("plan.pro.price");
  await page.locator("[data-replace-fact]").fill("plan.team.price");
  await page.locator("[data-preview-replace]").click();
  await expect(page.locator("[data-diff]")).not.toHaveCount(0);
});
