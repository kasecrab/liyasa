// ED-01's acceptance test.
//
// > Given a page with prose, a directive, a template chip, and a loop; when
// > opened in visual mode; then each becomes the specified ProseMirror node,
// > the preview of any block equals the build output byte for byte, and
// > editing prose re-serializes only that segment.
//
// **This spec has never run and is skipped.** Opening a draft needs
// `/_liyasa/editor/drafts/{id}` and the WebAssembly module served beside the
// bundle. Neither exists: `web/editor/src/api.ts` marks every editor route
// `unbuilt`, and `crates/liyasa-wasm` produces the module but nothing serves
// it. The mapping itself is covered today by `web/editor/test/corpus.test.ts`,
// against segmentations the real scanner produced, and ED-03's round trip by
// `tests/editor/ed_03_roundtrip.rs` over 1,000 generated documents.
//
// It is written out rather than left for later because the assertions are the
// requirement, and a row whose acceptance test does not exist cannot reach
// `done`. Remove the skip when the routes exist.

import { expect, test } from "@playwright/test";

test.skip(true, "waits on the editor's server routes and the served WebAssembly module");

const PAGE = "guides/limits.md";

test("each construct becomes the node ED-01 names", async ({ page }) => {
  await page.goto(`/_liyasa/editor/?page=${PAGE}&draft=new`);
  await expect(page.locator("[data-editor-surface] .block").first()).toBeVisible();
  await expect(page.locator(".block-paragraph").first()).toBeVisible();
  await expect(page.locator(".block-component").first()).toBeVisible();
  await expect(page.locator(".block-chip").first()).toBeVisible();
  await expect(page.locator(".block-logic_block").first()).toBeVisible();
  await expect(page.locator(".block-code").first()).toBeVisible();
});

test("a block's preview equals what the build produced for it", async ({ page, request }) => {
  await page.goto(`/_liyasa/editor/?page=${PAGE}&draft=new`);
  const previewed = await page.locator("[data-preview] .ly-callout").first().innerHTML();
  const built = await request.get("/guides/limits");
  expect(await built.text()).toContain(previewed);
});

test("editing prose re-serializes only that segment", async ({ page }) => {
  await page.goto(`/_liyasa/editor/?page=${PAGE}&draft=new`);
  const source = () => page.evaluate(() => (globalThis as unknown as { __source: string }).__source);
  const before = await source();
  await page.locator("[data-block='0.0']").fill("Edited.");
  await page.keyboard.press("Control+s");
  const after = await source();
  expect(after).not.toEqual(before);
  // Every byte from the directive onwards survived untouched.
  expect(after.slice(after.indexOf(":::"))).toEqual(before.slice(before.indexOf(":::")));
});
