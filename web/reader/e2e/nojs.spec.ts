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

    await expect(page.locator('[data-liyasa="page-title"]')).toBeVisible();
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
  // A route the sidebar actually lists: `/reference/cli` is linked from the
  // page body, not from the navigation, so it proves nothing about the nav.
  const target = '/guide/configuration';
  // On a phone the navigation is a drawer, and without a script the anchor
  // that opens it is the only way in. Reduced motion collapses the drawer's
  // transition to 1ms, so the link the reader then clicks is not a moving
  // target; it is also how a reader who asked for less motion gets there.
  const drawer = page.locator("[data-ly-drawer-trigger]");
  if (await drawer.isVisible()) {
    await page.emulateMedia({ reducedMotion: "reduce" });
    await page.goto("/");
    await drawer.click();
    await expect(page.locator("#ly-sidebar")).toBeInViewport();
    // The drawer slides for 0.2s even under reduced motion, because the `*`
    // reset in base.css loses to the class rule that sets the transition
    // (NEEDS-INPUT). Let it land rather than click a moving target.
    await page.waitForTimeout(400);
  }
  await page.locator(`nav a[href="${target}"], aside a[href="${target}"]`).first().click();
  await page.waitForURL(`**${target}`);
  await expect(page.locator('[data-liyasa="page-title"]')).toHaveText("Configuration");
});

test("the table of contents links into the page", async ({ viewport, page }) => {
  // The right rail is a desktop layout; `tests/web/nojs.rs` holds every page's
  // entries to resolving whatever the viewport is.
  test.skip((viewport?.width ?? 0) < 900, "the right rail is hidden on a phone");
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
  // With scripting off the browser parses `<noscript>` into real elements, so
  // the fallback is a link a reader can follow rather than inert text.
  await expect(page.locator('noscript a[href="/search"]')).toHaveCount(1);
});

// A class that sets `display` beats the browser's own `[hidden]` rule, so a
// control the markup hides is shown anyway and a reader without a script gets
// a button that does nothing. `tests/web/nojs.rs` asserts the same thing
// against the stylesheet, without a browser.
test("nothing the markup hides is shown anyway", async ({ page }) => {
  await page.goto("/");
  const shown = await page.$$eval("[hidden]", (nodes) =>
    nodes
      .filter((node) => getComputedStyle(node).display !== "none")
      .map((node) => node.className || node.tagName.toLowerCase()),
  );
  expect(shown).toEqual([]);
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
