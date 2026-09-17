// ED-05's acceptance test.
//
// > Given a loop block; when its expression is edited; then the expanded
// > preview updates for the selected context and the body remains a source
// > mini-editor.
//
// **This spec has never run and is skipped.** The caps, the preview context
// and what a chip's tooltip may claim are covered today by
// `web/editor/test/preview.test.ts`; the expansion itself needs the served
// WebAssembly module.

import { expect, test } from "@playwright/test";

test.skip(true, "waits on the served WebAssembly module");

test("editing a loop's expression updates the expanded preview", async ({ page }) => {
  await page.goto("/_liyasa/editor/?page=guides/limits.md&draft=new");
  await page.locator(".block-logic_block [data-expression]").first().fill("for row in plans");
  await expect(page.locator("[data-expanded]")).toContainText("Pro");
});

test("the loop's body stays a source mini-editor", async ({ page }) => {
  // ED-05 says WYSIWYG inside a loop body is deliberately not offered: a body
  // is frequently partial Markdown, such as a run of table rows.
  await page.goto("/_liyasa/editor/?page=guides/limits.md&draft=new");
  await expect(page.locator(".block-logic_block [data-body]").first()).toHaveAttribute(
    "data-source-editor",
    "",
  );
});

test("a loop past the cap draws the cap and says how many rows there are", async ({ page }) => {
  await page.goto("/_liyasa/editor/?page=guides/big-loop.md&draft=new");
  await expect(page.locator("[data-expanded] tr")).toHaveCount(50);
  await expect(page.locator("[data-show-all]")).toContainText("900");
});

test("a chip's tooltip names where its value came from", async ({ page }) => {
  await page.goto("/_liyasa/editor/?page=guides/limits.md&draft=new");
  const chip = page.locator(".block-chip").first();
  await chip.hover();
  await expect(page.locator("[data-chip-source]")).toContainText("from facts/");
});

test("the toolbar's context changes what a chip resolves to", async ({ page }) => {
  await page.goto("/_liyasa/editor/?page=guides/limits.md&draft=new");
  const chip = page.locator(".block-chip").first();
  const before = await chip.textContent();
  await page.locator("[data-context-version]").selectOption("2.0");
  await expect(chip).not.toHaveText(before ?? "");
});
