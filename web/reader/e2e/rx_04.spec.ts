// RX-04: links in the viewport are prefetched on hover or intersection,
// `Save-Data` turns it off, and a page change is a real navigation the browser
// transitions — there is no client router to do it instead.

import { expect, test } from "@playwright/test";

const PAGE = "/guide/install";

/** `<link rel=prefetch>` elements the runtime has added. */
async function prefetched(page: import("@playwright/test").Page): Promise<string[]> {
  return page.$$eval('link[rel="prefetch"]', (links) =>
    links.map((link) => new URL((link as HTMLLinkElement).href).pathname),
  );
}

// A route the sidebar does not list, so intersection has not already
// prefetched it by the time the pointer arrives. Every sidebar link is in view
// on a desktop viewport, which is the other half of RX-04 and is its own test.
const HOVER_ONLY = "/reference/cli";

test("a link the reader hovers is prefetched", async ({ page }) => {
  const requested: string[] = [];
  page.on("request", (request) => requested.push(new URL(request.url()).pathname));

  await page.goto("/");
  const link = page.locator(`main a[href="${HOVER_ONLY}"]`).first();
  await expect(link).toBeVisible();
  expect(await prefetched(page)).not.toContain(HOVER_ONLY);

  await link.hover();
  await expect.poll(async () => await prefetched(page)).toContain(HOVER_ONLY);
  expect(requested).toContain(HOVER_ONLY);
});

test("a link in the sidebar is prefetched when it comes into view", async ({ page }) => {
  await page.goto("/");
  await expect
    .poll(async () => (await prefetched(page)).length, { timeout: 5_000 })
    .toBeGreaterThan(0);
});

test("a reader who asked for less data is prefetched nothing", async ({ page }) => {
  await page.addInitScript(() => {
    Object.defineProperty(navigator, "connection", {
      configurable: true,
      value: { saveData: true },
    });
  });
  const requested: string[] = [];
  page.on("request", (request) => requested.push(new URL(request.url()).pathname));

  await page.goto("/");
  const link = page.locator('main a[href="/guide/configuration"]').first();
  await link.hover();
  await page.waitForTimeout(500);

  expect(await prefetched(page)).toHaveLength(0);
  expect(requested).not.toContain("/guide/configuration");
});

test("the same page is prefetched once, however often it is hovered", async ({ page }) => {
  await page.goto("/");
  const link = page.locator('main a[href="/guide/install"]').first();
  await link.hover();
  await expect.poll(async () => await prefetched(page)).toContain("/guide/install");
  await page.mouse.move(0, 0);
  await link.hover();
  const links = await prefetched(page);
  expect(links.filter((href) => href === "/guide/install")).toHaveLength(1);
});

test("the browser is told to transition between pages", async ({ page }) => {
  await page.goto(PAGE);
  const declared = await page.evaluate(() =>
    Array.from(document.adoptedStyleSheets).some((sheet) =>
      Array.from(sheet.cssRules).some((rule) => rule.cssText.includes("view-transition")),
    ),
  );
  expect(declared).toBe(true);
});

test("a page change is a navigation, not a swapped fragment", async ({ page }) => {
  await page.goto("/");
  await page.locator('a[href="/guide/install"]').first().click();
  await page.waitForURL("**/guide/install");

  const kind = await page.evaluate(
    () =>
      (performance.getEntriesByType("navigation")[0] as PerformanceNavigationTiming | undefined)
        ?.type,
  );
  expect(kind).toBe("navigate");
  // A router would have kept the first document; a navigation replaces it.
  await expect(page.locator('[data-liyasa="page-title"]')).toHaveText("Install Liyasa");
});

test("a reader who asked for less motion gets no transition", async ({ page }) => {
  await page.emulateMedia({ reducedMotion: "reduce" });
  await page.goto(PAGE);
  const declared = await page.evaluate(() =>
    Array.from(document.adoptedStyleSheets).some((sheet) =>
      Array.from(sheet.cssRules).some((rule) => rule.cssText.includes("view-transition")),
    ),
  );
  expect(declared).toBe(false);
});
