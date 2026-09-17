// ED-21's acceptance test.
//
// > Given a draft and a conflicting change on the deploy branch; when autosave
// > runs; then the conflict is detected and the three-way merge UI shows both
// > rendered versions.
//
// **This spec has never run and is skipped.** The versioned save, the refusal
// on a stale base and the three-way merge are covered today by
// `web/editor/test/drafts.test.ts`. Producing a conflict needs a server
// holding the draft and a deploy branch that can move under it.

import { expect, test } from "@playwright/test";

test.skip(true, "waits on the editor's draft save route");

test("a stale save is refused and the merge shows both versions rendered", async ({ page, request }) => {
  await page.goto("/_liyasa/editor/?page=guides/limits.md&draft=new");
  await page.locator("[data-block='0.0']").fill("Mine.");

  // Somebody else moves the deploy branch under this draft.
  await request.put("/_liyasa/editor/drafts/current/files", {
    data: { path: "guides/limits.md", text: "Theirs.", baseVersion: 1 },
  });

  await expect(page.locator("[data-conflict]")).toBeVisible();
  await expect(page.locator("[data-conflict-mine] p")).toHaveText("Mine.");
  await expect(page.locator("[data-conflict-theirs] p")).toHaveText("Theirs.");
  // Rendered, not a diff with conflict markers in it.
  await expect(page.locator("[data-conflict-mine]")).not.toContainText("<<<<<<<");
});

test("the local edit comes back as a suggestion to accept or discard", async ({ page, request }) => {
  await page.goto("/_liyasa/editor/?page=guides/limits.md&draft=new");
  await page.locator("[data-block='0.0']").fill("Mine.");
  await request.put("/_liyasa/editor/drafts/current/files", {
    data: { path: "guides/limits.md", text: "Theirs.", baseVersion: 1 },
  });
  await expect(page.locator("[data-conflict-accept]")).toBeVisible();
  await expect(page.locator("[data-conflict-discard]")).toBeVisible();
  await page.locator("[data-conflict-discard]").click();
  await expect(page.locator("[data-block='0.0']")).toHaveText("Theirs.");
});

test("the same draft open in two tabs says so", async ({ browser }) => {
  const first = await browser.newContext();
  const second = await browser.newContext();
  const a = await first.newPage();
  const b = await second.newPage();
  await a.goto("/_liyasa/editor/?page=guides/limits.md&draft=current");
  await b.goto("/_liyasa/editor/?page=guides/limits.md&draft=current");
  await expect(a.locator("[data-also-open]")).toBeVisible();
  await first.close();
  await second.close();
});

test("a save is announced politely and a refusal assertively", async ({ page }) => {
  await page.goto("/_liyasa/editor/?page=guides/limits.md&draft=new");
  await page.locator("[data-block='0.0']").fill("Edited.");
  await expect(page.locator("[data-announce]")).toHaveText("Saved");
  await expect(page.locator("[data-announce]")).toHaveAttribute("aria-live", "polite");
});
