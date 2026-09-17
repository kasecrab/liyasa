// ED-71's acceptance test.
//
// > Given the "update a number" task with a fact used on four pages; when the
// > number is changed; then the proposal updates the fact source and shows the
// > four affected pages; the usability study (NFR-52) completes the task in
// > under three minutes.
//
// **This spec has never run and is skipped.** The five tasks, and the rule
// that "update a number" writes the *fact* rather than the pages, are covered
// today by `web/editor/test/tasks.test.ts`. The usability half belongs to
// NFR-52 and is a study with people in it: nothing in this file can stand in
// for it, and a browser timing itself would be this package marking its own
// homework.

import { expect, test } from "@playwright/test";

test.skip(true, "waits on the editor's draft route; the usability half is NFR-52's study");

test("update a number writes the fact and shows the four pages", async ({ page }) => {
  await page.goto("/_liyasa/editor/?draft=new#/tasks/update-a-number");
  await page.locator("[data-fact]").selectOption("plan.pro.requests");
  await page.locator("[data-value]").fill("250000");
  await page.locator("[data-preview-task]").click();

  await expect(page.locator("[data-affected] li")).toHaveCount(4);
  await expect(page.locator("[data-writes]")).toContainText("facts/plan.json");
  // The pages are listed, not edited: the number lives in the fact, and
  // editing the pages would leave the fact stale.
  await expect(page.locator("[data-writes]")).not.toContainText("guides/limits.md");
});

test("the task ends in a proposal rather than a publish", async ({ page }) => {
  await page.goto("/_liyasa/editor/?draft=new#/tasks/update-a-number");
  await page.locator("[data-fact]").selectOption("plan.pro.requests");
  await page.locator("[data-value]").fill("250000");
  await page.locator("[data-preview-task]").click();
  await page.locator("[data-submit-task]").click();
  await expect(page.locator("[data-proposal]")).toBeVisible();
  await expect(page.locator("[data-action='publish']")).toHaveCount(0);
});

test("every task the requirement names is offered", async ({ page }) => {
  await page.goto("/_liyasa/editor/?draft=new#/tasks");
  for (const task of [
    "update-a-number",
    "rename-everywhere",
    "replace-a-screenshot",
    "add-a-faq-entry",
    "record-a-changelog-entry",
  ]) {
    await expect(page.locator(`[data-task='${task}']`)).toBeVisible();
  }
});
