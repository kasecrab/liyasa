// ED-02's acceptance test.
//
// > Given the same page in source mode; when a directive prop is mistyped;
// > then the diagnostic from the WebAssembly validator appears inline within
// > 100 ms with the same code the CLI would give.
//
// **This spec has never run and is skipped.** The diagnostic comes from the
// served WebAssembly module, which nothing serves yet. What this row is about
// besides the round trip — where a diagnostic lands, what the tokens are, what
// completes where — is covered today by `web/editor/test/source.test.ts`, and
// the code itself by `crates/liyasa-wasm`'s own suite.

import { expect, test } from "@playwright/test";

test.skip(true, "waits on the served WebAssembly module");

test("a mistyped prop is reported inline, with the code the CLI would give", async ({ page }) => {
  await page.goto("/_liyasa/editor/?page=guides/limits.md&draft=new");
  await page.locator("[data-mode-switch]").click();
  await page.locator("[data-source-editor]").fill(':::note{titel="Heads up"}\nbody\n:::\n');

  const problem = page.locator(".problem").first();
  // NFR-05's keystroke budget: the diagnostic is part of the same frame's work.
  await expect(problem).toBeVisible({ timeout: 100 });
  await expect(problem.locator(".code")).toHaveText("W0316");
  // ED-73: the plain-language variant, not the registry title.
  await expect(problem.locator(".what")).toContainText("does not have a setting by that name");
});

test("highlighting and the preview agree about what a construct is", async ({ page }) => {
  await page.goto("/_liyasa/editor/?page=guides/limits.md&draft=new");
  await page.locator("[data-mode-switch]").click();
  await expect(page.locator("[data-token='directive']").first()).toBeVisible();
  await expect(page.locator("[data-token='template-output']").first()).toBeVisible();
  await expect(page.locator("[data-preview]")).toBeVisible();
});

test("front matter is validated against its schema in the same pane", async ({ page }) => {
  await page.goto("/_liyasa/editor/?page=guides/limits.md&draft=new");
  await page.locator("[data-mode-switch]").click();
  await page.locator("[data-source-editor]").fill("---\ndraft: yes please\n---\n\nbody\n");
  await expect(page.locator(".problem .code").first()).toHaveText("E0102");
});
