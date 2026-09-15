// RX-62: `mod+shift+c` copies the page as Markdown, through the same action
// the menu item runs (`plan/rfcs/1103-copy-markdown-shortcut.md`).

import { expect, test } from "@playwright/test";

const PAGE = "/guide/install";
const MOD = process.platform === "darwin" ? "Meta" : "Control";
const BUTTON = '[data-ly-action="copy-markdown"]';

test.use({ permissions: ["clipboard-read", "clipboard-write"] });

// RX-100 gives the action the page's *absolute* Markdown URL, and the site
// under test is built for its published origin rather than for this server, so
// `copy.js` would fetch the real host and copy whatever it answered. Point the
// Markdown twin back at the server under test, which is what the URL resolves
// to on a deployed site.
test.beforeEach(async ({ page, baseURL }) => {
  await page.route(/\.md(\?.*)?$/, async (route) => {
    const { pathname } = new URL(route.request().url());
    // Node's own fetch, not `page.request`: the reply outlives the test that
    // only waited for the request to be made.
    const response = await fetch(`${baseURL}${pathname}`);
    await route.fulfill({
      status: response.status,
      headers: { "content-type": "text/markdown; charset=utf-8" },
      body: Buffer.from(await response.arrayBuffer()),
    });
  });
});

async function clipboard(page: import("@playwright/test").Page): Promise<string> {
  return page.evaluate(() => navigator.clipboard.readText());
}

test("the shortcut copies the page's markdown", async ({ page }) => {
  await page.goto(PAGE);
  await page.evaluate(() => navigator.clipboard.writeText(""));

  await page.keyboard.press(`${MOD}+Shift+C`);
  await expect.poll(async () => await clipboard(page)).toContain("# Install Liyasa");
});

test("the shortcut and the menu item copy the same text", async ({ page }) => {
  await page.goto(PAGE);
  await page.locator("[data-ly-actions-trigger]").click();
  await page.locator(BUTTON).click();
  await expect.poll(async () => (await clipboard(page)).length).toBeGreaterThan(0);
  const byMenu = await clipboard(page);

  await page.evaluate(() => navigator.clipboard.writeText(""));
  await page.keyboard.press(`${MOD}+Shift+C`);
  await expect.poll(async () => await clipboard(page)).toBe(byMenu);
});

test("the markdown is fetched, never inlined into the page", async ({ page }) => {
  const fetched: string[] = [];
  page.on("request", (request) => fetched.push(new URL(request.url()).pathname));

  await page.goto(PAGE);
  const html = await page.content();
  expect(html).not.toContain("# Install Liyasa");

  await page.keyboard.press(`${MOD}+Shift+C`);
  await expect.poll(() => fetched).toContain(`${PAGE}.md`);
});

test("the shortcut announces the copy, as the menu item does", async ({ page }) => {
  await page.goto(PAGE);
  await page.keyboard.press(`${MOD}+Shift+C`);
  await expect(page.locator("#ly-live-region")).toContainText("Copied");
});

test("a page with no markdown action leaves the chord to the browser", async ({ page }) => {
  await page.goto(PAGE);
  await page.evaluate((selector) => {
    document.querySelectorAll(selector).forEach((node) => node.remove());
  }, BUTTON);

  const prevented = await page.evaluate(
    (mod) =>
      new Promise<boolean>((resolve) => {
        document.addEventListener(
          "keydown",
          (event) => resolve(event.defaultPrevented),
          { once: true },
        );
        document.dispatchEvent(
          new KeyboardEvent("keydown", {
            key: "C",
            shiftKey: true,
            ctrlKey: mod === "Control",
            metaKey: mod === "Meta",
            cancelable: true,
            bubbles: true,
          }),
        );
      }),
    MOD,
  );
  expect(prevented).toBe(false);
});
