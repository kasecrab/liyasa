// ED-11's acceptance test.
//
// > Given the front matter form; when a value invalid per the schema is
// > entered; then the field shows the schema error and save is blocked.
//
// **This spec has never run and is skipped.** The form's generation from
// `schemas/frontmatter.json`, its validation in both directions, and its
// byte-preserving write are covered today by
// `web/editor/test/frontmatter.test.ts`. Opening a draft needs the server.

import { expect, test } from "@playwright/test";

test.skip(true, "waits on the editor's draft route");

test("a value the schema rejects shows the error on its field and blocks the save", async ({ page }) => {
  await page.goto("/_liyasa/editor/?page=guides/limits.md&draft=new");
  await page.locator("[data-open-frontmatter]").click();
  await page.locator("[data-field='draft'] input").fill("yes please");
  await expect(page.locator("[data-field-error='draft']")).toContainText("expects boolean");
  await expect(page.locator("[data-action='save']")).toBeDisabled();
});

test("the advanced keys sit behind a disclosure", async ({ page }) => {
  await page.goto("/_liyasa/editor/?page=guides/limits.md&draft=new");
  await page.locator("[data-open-frontmatter]").click();
  await expect(page.locator("[data-field='title']")).toBeVisible();
  await expect(page.locator("[data-field='verify']")).toBeHidden();
  await page.locator("[data-advanced-frontmatter]").click();
  await expect(page.locator("[data-field='verify']")).toBeVisible();
});

test("every field carries its own help", async ({ page }) => {
  await page.goto("/_liyasa/editor/?page=guides/limits.md&draft=new");
  await page.locator("[data-open-frontmatter]").click();
  await page.locator("[data-advanced-frontmatter]").click();
  const fields = page.locator("[data-field]");
  const count = await fields.count();
  expect(count).toBeGreaterThan(20);
  for (let at = 0; at < count; at += 1) {
    await expect(fields.nth(at).locator("[data-field-help]")).not.toHaveText("");
  }
});

test("a correct value saves and leaves the rest of the block alone", async ({ page, request }) => {
  await page.goto("/_liyasa/editor/?page=guides/limits.md&draft=new");
  await page.locator("[data-open-frontmatter]").click();
  await page.locator("[data-field='description'] textarea").fill("The caps a project runs under.");
  await page.locator("[data-action='save']").click();
  const file = await (await request.get("/_liyasa/editor/fs/guides/limits.md")).text();
  expect(file).toContain("description: The caps a project runs under.");
  expect(file).toContain("title: Limits");
});
