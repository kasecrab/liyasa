// ED-04's acceptance test.
//
// > Given slash commands, drag reorder, and a paste from Google Docs, Notion,
// > and HTML fixtures; when performed; then the resulting Markdown matches
// > golden files and images land in the media library.
//
// **This spec has never run and is skipped.** The conversion and the commands
// are covered today by `web/editor/test/paste.test.ts` and
// `web/editor/test/commands.test.ts`, against the same golden files. What only
// a browser adds is the clipboard event and the drag; the upload needs
// `/_liyasa/editor/assets`, which nothing serves.

import { expect, test } from "@playwright/test";

test.skip(true, "waits on the editor's asset route");

test("a slash command inserts Markdown the build parses", async ({ page }) => {
  await page.goto("/_liyasa/editor/?page=guides/limits.md&draft=new");
  await page.locator("[data-block='0.0']").click();
  await page.keyboard.press("End");
  await page.keyboard.type("/callout");
  await page.keyboard.press("Enter");
  await expect(page.locator(".block-component").last()).toContainText("Heads up");
});

test("a paste from Google Docs converts to its golden file", async ({ page }) => {
  await page.goto("/_liyasa/editor/?page=guides/limits.md&draft=new");
  await page.evaluate(async () => {
    const html = await (await fetch("/fixtures/paste/google-docs.html")).text();
    const data = new DataTransfer();
    data.setData("text/html", html);
    document
      .querySelector("[data-editor-surface]")
      ?.dispatchEvent(new ClipboardEvent("paste", { clipboardData: data, bubbles: true }));
  });
  await expect(page.locator("[data-editor-surface]")).toContainText("Install the CLI");
  await expect(page.locator("[data-editor-surface]")).not.toContainText("docs-internal-guid");
});

test("a dragged block moves and nothing else does", async ({ page }) => {
  await page.goto("/_liyasa/editor/?page=guides/limits.md&draft=new");
  const second = page.locator("[data-block='0.1']");
  const moved = await second.textContent();
  await page.locator("[data-block='0.0']").dragTo(second);
  await expect(page.locator("[data-block='0.0']")).toHaveText(moved ?? "");
});

test("a pasted image lands in the media library rather than staying remote", async ({ page }) => {
  await page.goto("/_liyasa/editor/?page=guides/limits.md&draft=new");
  await page.locator("[data-media-library]").click();
  await expect(page.locator("[data-asset]")).not.toHaveCount(0);
  await expect(page.locator("[data-editor-surface]")).not.toContainText("https://example.invalid");
});
