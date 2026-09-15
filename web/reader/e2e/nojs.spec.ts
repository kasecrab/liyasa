// THM-31: every page is readable and navigable with JavaScript disabled.
//
// The runtime is progressive enhancement, so this suite is the definition of
// what "enhancement" means here: with the bundle absent, a reader still gets
// the page, the navigation, the table of contents, the Markdown twin, and a
// way into search.

import { expect, test } from "@playwright/test";

test.use({ javaScriptEnabled: false });

const ROUTES = ["/", "/guide/install", "/guide/configuration", "/reference/cli"];

for (const route of ROUTES) {
  test(`\`${route}\` is readable without javascript`, async ({ page }) => {
    await page.goto(route);

    await expect(page.locator("main h1")).toBeVisible();
    await expect(page.locator("main")).not.toBeEmpty();
    // The bootstrap marks the document once it runs; nothing here may depend
    // on that having happened.
    await expect(page.locator("html")).not.toHaveAttribute("data-ly-js", "true");
    // The Markdown twin an agent or a reader can follow (RX-60).
    await expect(page.locator('link[rel="alternate"][type="text/markdown"]')).toHaveCount(1);
  });
}

test("the navigation moves between pages", async ({ page }) => {
  await page.goto("/");
  await page.locator('nav a[href="/reference/cli"], aside a[href="/reference/cli"]').first().click();
  await page.waitForURL("**/reference/cli");
  await expect(page.locator("main h1")).toHaveText("CLI reference");
});

test("the table of contents links into the page", async ({ page }) => {
  await page.goto("/guide/configuration");
  const entries = page.locator('[data-liyasa="toc"] a');
  await expect(entries.first()).toBeVisible();
  const href = await entries.first().getAttribute("href");
  expect(href).toMatch(/^#/);
  await expect(page.locator(`${href}`)).toHaveCount(1);
});

test("the skip link reaches the page", async ({ page }) => {
  await page.goto("/guide/install");
  await expect(page.locator('[data-liyasa="skip-link"]')).toHaveAttribute("href", "#ly-main");
  await expect(page.locator("#ly-main")).toHaveCount(1);
});

test("the sidebar opens on a phone without a script", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto("/guide/install");
  const trigger = page.locator("[data-ly-drawer-trigger]");
  await expect(trigger).toHaveAttribute("href", "#ly-sidebar");
  await trigger.click();
  await expect(page.locator("#ly-sidebar")).toBeVisible();
});

test("search offers a page rather than a dead button", async ({ page }) => {
  await page.goto("/");
  // The button the overlay uses is hidden until its module runs; the fallback
  // is a link to the search page.
  await expect(page.locator("[data-ly-search-trigger]")).toBeHidden();
  await expect(page.locator('noscript >> text=Search')).toHaveCount(1);
});

test("no page hides its content behind a script", async ({ page }) => {
  for (const route of ROUTES) {
    await page.goto(route);
    const hidden = await page.locator("main [hidden]").count();
    expect(hidden, `\`${route}\` hides part of the page`).toBe(0);
    const text = (await page.locator("main").innerText()).trim();
    expect(text.length, `\`${route}\` renders no text`).toBeGreaterThan(200);
  }
});
