// ED-70's acceptance test.
//
// > Given a signed-in member on a published page; when "Suggest an edit" is
// > clicked; then the editor opens on that page in a new draft within 2 s.
//
// **This spec has never run and is skipped.** The URL the button goes to,
// including the branch name it asks for and the fact that a route with slashes
// needs no escaping scheme of its own, is covered today by
// `web/editor/test/tasks.test.ts`. The button itself is a page action on the
// reader's page, and the editor it opens needs the draft route.

import { expect, test } from "@playwright/test";

test.skip(true, "waits on the editor's draft route and the reader's page action");

test("Suggest an edit opens the editor on that page in a new draft", async ({ page }) => {
  await page.goto("/guides/limits");
  const started = Date.now();
  await page.locator("[data-page-action='suggest']").click();
  await expect(page.locator("[data-editor-surface]")).toBeVisible({ timeout: 2_000 });
  expect(Date.now() - started).toBeLessThan(2_000);

  const query = new URL(page.url()).searchParams;
  expect(query.get("page")).toBe("/guides/limits");
  expect(query.get("draft")).toBe("new");
  expect(query.get("branch")).toMatch(/^liyasa\/[a-z0-9-]+\/guides-limits$/);
});

test("a quick fix edits one block without leaving the page", async ({ page }) => {
  await page.goto("/guides/limits");
  await page.locator("[data-page-action='quick-fix']").click();
  await page.locator("[data-quick-fix-block]").first().fill("A corrected sentence.");
  await page.locator("[data-quick-fix-submit]").click();
  await expect(page).toHaveURL(/\/guides\/limits$/);
  await expect(page.locator("[data-quick-fix-done]")).toContainText("suggested");
});

test("a quick fix is still a proposal, not a publish", async ({ page }) => {
  await page.goto("/guides/limits");
  await page.locator("[data-page-action='quick-fix']").click();
  await page.locator("[data-quick-fix-block]").first().fill("A corrected sentence.");
  await page.locator("[data-quick-fix-submit]").click();
  await expect(page.locator("[data-quick-fix-done] a")).toHaveAttribute("href", /\/reviews\//);
});
