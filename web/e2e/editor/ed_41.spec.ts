// ED-41's acceptance test.
//
// > Given three suggested blocks; when one is rejected; then only the other
// > two are applied.
//
// **This spec has never run and is skipped.** The same guarantee is asserted
// today by `web/editor/test/agent.test.ts` against the model: `acceptedEdits`
// is the only function that turns an agent's output into something writable
// and it reads `status`, so a pending or rejected suggestion produces no edit.
// A browser adds the buttons.

import { expect, test } from "@playwright/test";

test.skip(true, "waits on the editor's agent route and a mock model");

test("rejecting one of three applies the other two", async ({ page, request }) => {
  await page.goto("/_liyasa/editor/?page=guides/limits.md&draft=new");
  await page.locator("[data-agent='restructure']").click();
  await expect(page.locator(".suggestion")).toHaveCount(3);

  const rejected =
    (await page.locator(".suggestion").nth(1).locator(".suggestion-after").textContent()) ?? "never";
  await page.locator(".suggestion").nth(1).locator("[data-reject]").click();
  await page.locator("[data-accept-remaining]").click();

  await expect(page.locator("[data-applied-count]")).toHaveText("2");
  const after = await (await request.get("/_liyasa/editor/fs/guides/limits.md")).text();
  expect(after).not.toContain(rejected);
});

test("nothing is applied while every suggestion is still pending", async ({ page, request }) => {
  const read = async () => (await request.get("/_liyasa/editor/fs/guides/limits.md")).text();
  const before = await read();
  await page.goto("/_liyasa/editor/?page=guides/limits.md&draft=new");
  await page.locator("[data-agent='restructure']").click();
  await expect(page.locator(".suggestion")).toHaveCount(3);
  expect(await read()).toEqual(before);
});

test("rejecting all three writes nothing at all", async ({ page, request }) => {
  const read = async () => (await request.get("/_liyasa/editor/fs/guides/limits.md")).text();
  const before = await read();
  await page.goto("/_liyasa/editor/?page=guides/limits.md&draft=new");
  await page.locator("[data-agent='restructure']").click();
  await page.locator("[data-reject-all]").click();
  expect(await read()).toEqual(before);
});

test("a suggestion is reachable and decidable from the keyboard alone", async ({ page }) => {
  await page.goto("/_liyasa/editor/?page=guides/limits.md&draft=new");
  await page.locator("[data-agent='restructure']").click();
  await page.keyboard.press("j");
  await page.keyboard.press("a");
  await expect(page.locator(".suggestion").first()).toHaveAttribute("data-status", "accepted");
});
