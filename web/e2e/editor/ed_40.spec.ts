// ED-40's acceptance test (mock model).
//
// > Given a selection and "rewrite shorter"; when the suggestion arrives; then
// > it is shown as a tracked change, nothing is written until accepted, and the
// > validator ran on the suggestion.
//
// **This spec has never run and is skipped.** The operations, what the run
// carries (ED-42's `AGENTS.md`, style guide and verification policy) and the
// rule that a suggestion the validator rejects is withheld are covered today
// by `web/editor/test/agent.test.ts`. A run needs `/_liyasa/editor/agent/run`,
// which nothing serves.

import { expect, test } from "@playwright/test";

test.skip(true, "waits on the editor's agent route and a mock model");

test("a rewrite arrives as a tracked change and writes nothing", async ({ page, request }) => {
  const read = async () => (await request.get("/_liyasa/editor/fs/guides/limits.md")).text();
  const before = await read();

  await page.goto("/_liyasa/editor/?page=guides/limits.md&draft=new");
  await page.locator("[data-block='0.0']").click();
  await page.locator("[data-agent='rewrite']").click();
  await page.locator("[data-variant='shorter']").click();

  await expect(page.locator(".suggestion").first()).toBeVisible();
  await expect(page.locator(".suggestion-before").first()).toBeVisible();
  await expect(page.locator(".suggestion-after").first()).toBeVisible();
  expect(await read()).toEqual(before);
});

test("the validator ran on the suggestion before it was offered", async ({ page }) => {
  await page.goto("/_liyasa/editor/?page=guides/limits.md&draft=new");
  await page.locator("[data-block='0.0']").click();
  await page.locator("[data-agent='rewrite']").click();
  await page.locator("[data-variant='shorter']").click();
  await expect(page.locator("[data-suggestion-checked]")).toHaveAttribute(
    "data-suggestion-checked",
    "true",
  );
});

test("a suggestion the validator rejected is withheld and counted", async ({ page }) => {
  await page.goto("/_liyasa/editor/?page=guides/broken-suggestion.md&draft=new");
  await page.locator("[data-agent='restructure']").click();
  await expect(page.locator("[data-withheld]")).toContainText("1 suggestion was not offered");
});

test("the run carried the project's own rules", async ({ page }) => {
  const runs: unknown[] = [];
  await page.route("**/_liyasa/editor/agent/run", async (route) => {
    runs.push(route.request().postDataJSON());
    await route.continue();
  });
  await page.goto("/_liyasa/editor/?page=guides/limits.md&draft=new");
  await page.locator("[data-agent='rewrite']").click();
  await page.locator("[data-variant='shorter']").click();
  expect(JSON.stringify(runs)).toContain("styleGuide");
  expect(JSON.stringify(runs)).toContain("agents");
});
