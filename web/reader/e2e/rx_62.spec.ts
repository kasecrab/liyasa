// RX-62: `mod+shift+c` copies the page as Markdown, through the same action
// the menu item runs (`plan/rfcs/1103-copy-markdown-shortcut.md`).

import { expect, test } from "@playwright/test";

const PAGE = "/guide/install";
const MOD = process.platform === "darwin" ? "Meta" : "Control";
const BUTTON = '[data-ly-action="copy-markdown"]';

test.use({ permissions: ["clipboard-read", "clipboard-write"] });

// The twin the reader dereferences is a path, so the browser resolves it
// against whatever host served the page (RFC 0505). Nothing here intercepts
// it: the fetch goes to the server under test because the markup says so, and
// this test is what keeps it that way — an absolute URL here would send a
// preview, a staging host or a mirror to the published origin instead.
test("the action the reader dereferences stays on the serving host", async ({ page }) => {
  await page.goto(PAGE);
  const copy = await page.locator(BUTTON).getAttribute("data-ly-copy-url");
  expect(copy).toBe(`${PAGE}.md`);
  const view = await page.locator('[data-ly-action="view-markdown"]').getAttribute("href");
  expect(view).toBe(`${PAGE}.md`);
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
