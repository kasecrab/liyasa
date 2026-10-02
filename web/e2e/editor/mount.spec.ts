// The mount: that something opens the panes, not that the panes render.
//
// Every renderer under `web/editor/src/view/` had a unit test and no caller for
// several days. `renderSurface`, `renderProperties`, `renderSourcePopover` and
// `renderFrontmatterForm` were all reachable from `MODULES`, which made them
// look wired — the registry exists so the bundler walks the whole graph, and a
// side effect is that grepping for a renderer finds a hit. A renderer test says
// nothing about whether any author action reaches it.
//
// So this spec clicks. It supplies the draft the way the server will — embedded
// in the shell as `<script type="application/json" data-draft>`, which the route
// can do for free because it has the parsed document in hand — and then drives
// the handles a person would press.
//
// What it does not claim: that a draft can be *fetched*. `api.ts` marks every
// editor route `unbuilt` and nothing serves the WebAssembly module, so the
// specs that need a real draft (`ed_01`, `ed_02`, `ed_11`) are still skipped.
// The difference is that those wait on another package and this one does not.

import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { expect, test } from "@playwright/test";

import { startEditorServer } from "./editor-server.ts";
import type { EditorServer } from "./editor-server.ts";

const HERE = dirname(fileURLToPath(import.meta.url));
const CORPUS: { path: string; source: string; document: unknown }[] = JSON.parse(
  readFileSync(resolve(HERE, "../../editor/test/fixtures/segments.json"), "utf8"),
);
// The schema travels with the draft, so the fixture carries the real one rather
// than a stand-in: the form under test is the form the project generates.
const SCHEMA = JSON.parse(readFileSync(resolve(HERE, "../../../schemas/frontmatter.json"), "utf8"));

/** The corpus page with a component, a chip and a loop in it. */
const PAGE = CORPUS.find((each) => each.path === "limits.md") ?? CORPUS[0]!;
/**
 * The page of constructs the visual editor does not model.
 *
 * ED-03(c)'s test skipped for a day against `limits.md`, whose only opaque node
 * is a loop *body* — rendered as a source mini-editor by design, so it never
 * produces an "edit as source" handle. This page exists so the popover has
 * something real to open onto.
 */
const OPAQUE = CORPUS.find((each) => each.path === "opaque.md");

let server: EditorServer;

test.beforeAll(async () => {
  server = await startEditorServer();
});

test.afterAll(async () => {
  await server.close();
});

/**
 * Loads the shell with a draft embedded, exactly as the route will serve it.
 *
 * The injection happens in `page.route` rather than after load, because the
 * thing under test is that the shell reads an embedded draft *while mounting*.
 * Setting the element afterwards would prove something else.
 */
async function openWithDraft(
  page: import("@playwright/test").Page,
  draft: { path: string; source: string; document: unknown } = PAGE,
): Promise<void> {
  const payload = JSON.stringify({
    path: draft.path,
    source: draft.source,
    document: draft.document,
    schema: SCHEMA,
  });
  await page.route(`${server.url}`, async (route) => {
    const response = await route.fetch();
    const html = await response.text();
    await route.fulfill({
      response,
      body: html.replace(
        "</body>",
        `<script type="application/json" data-draft>${payload.replace(/</g, "\\u003c")}</script></body>`,
      ),
    });
  });
  await page.goto(server.url);
  await expect(page.locator("[data-editor-surface] [data-block]").first()).toBeVisible();
}

test("an embedded draft is drawn: the surface has a block per node", async ({ page }) => {
  await openWithDraft(page);
  const blocks = page.locator("[data-editor-surface] [data-block]");
  expect(await blocks.count()).toBeGreaterThan(1);
  await expect(page.locator("[data-editor-surface]")).not.toContainText("Loading this draft");
  await expect(page.locator("[data-draft-status]")).toContainText(PAGE.path);
});

test("ED-01: a component's handle opens its properties with that component's fields", async ({ page }) => {
  await openWithDraft(page);
  const handle = page.locator("[data-open-properties]").first();
  const block = await handle.getAttribute("data-open-properties");
  await expect(page.locator("[data-panel] [data-properties]")).toHaveCount(0);
  await handle.click();
  const pane = page.locator("[data-panel] [data-properties]");
  await expect(pane).toHaveAttribute("data-properties", block!);
  expect(await pane.locator("[data-prop]").count()).toBeGreaterThan(0);
  await expect(pane.locator("[data-unserved]")).toHaveCount(0);
});

test("the same handle closes it and a different block's handle swaps it", async ({ page }) => {
  await openWithDraft(page);
  const handles = page.locator("[data-open-properties]");
  const count = await handles.count();
  const first = await handles.nth(0).getAttribute("data-open-properties");

  await handles.nth(0).click();
  await expect(page.locator("[data-panel] [data-properties]")).toHaveAttribute("data-properties", first!);
  await handles.nth(0).click();
  await expect(page.locator("[data-panel] [data-properties]")).toHaveCount(0);

  test.skip(count < 2, "this fixture has one component");
  const second = await handles.nth(1).getAttribute("data-open-properties");
  await handles.nth(0).click();
  await handles.nth(1).click();
  await expect(page.locator("[data-panel] [data-properties]")).toHaveAttribute("data-properties", second!);
});

test("ED-03(c): the source popover opens over its own block, carrying its bytes", async ({ page }) => {
  expect(OPAQUE, "the fixture has opaque.md").toBeTruthy();
  await openWithDraft(page, OPAQUE!);
  const opaque = page.locator("[data-edit-source]").first();
  await expect(opaque).toHaveCount(1);
  const block = await opaque.getAttribute("data-edit-source");
  await opaque.click();
  // Inside the block, not in the side column: ED-03(c) calls it a popover.
  const popover = page.locator(`[data-block="${block}"] [data-source-popover]`);
  await expect(popover).toHaveCount(1);
  await expect(popover.locator("[data-source-editor]")).toHaveValue(/\S/);
  await expect(popover.locator("[data-prop]")).toHaveCount(0);
});

test("ED-11: Page settings opens the generated form with the draft's own values", async ({ page }) => {
  await openWithDraft(page);
  await page.locator("[data-open-frontmatter]").click();
  const form = page.locator("[data-panel] [data-frontmatter]");
  await expect(form).toHaveCount(1);
  expect(await form.locator("[data-field]").count()).toBeGreaterThan(5);
  await expect(form.locator("[data-unserved]")).toHaveCount(0);
});

test("with no draft, Page settings says the draft is missing rather than drawing an empty form", async ({ page }) => {
  // The state the editor is actually in today, and the one that must not lie: an
  // empty form reads as "this page has no settings".
  await page.goto(server.url);
  await page.locator("[data-open-frontmatter]").click();
  const panel = page.locator("[data-panel]");
  await expect(panel.locator("[data-unserved]")).toHaveCount(1);
  await expect(panel.locator("form")).toHaveCount(0);
  await expect(panel).toContainText("draft route is not served");
});

test("opening a pane announces it, so the change is not silent", async ({ page }) => {
  await openWithDraft(page);
  await page.locator("[data-open-properties]").first().click();
  // Names the component, because "Properties opened." twice in a row says the
  // same thing about two different blocks.
  await expect(page.locator("[data-announce]")).toContainText(/Properties for \w+\./);
  await page.locator("[data-open-frontmatter]").click();
  await expect(page.locator("[data-announce]")).toContainText("Page settings opened");
});

test("Escape closes an open pane and gives focus back to the control that opened it", async ({ page }) => {
  await openWithDraft(page);
  const handle = page.locator("[data-open-frontmatter]");
  await handle.click();
  await expect(page.locator("[data-panel] [data-frontmatter]")).toHaveCount(1);
  await page.keyboard.press("Escape");
  await expect(page.locator("[data-panel] [data-frontmatter]")).toHaveCount(0);
  await expect(handle).toBeFocused();
});

// --- ED-02: the mode switch ---------------------------------------------------
//
// The button has been in the shell since its first commit with nothing bound to
// it, and `renderToolbar` advertised `aria-keyshortcuts="Control+E"` for a
// shortcut that did nothing. Both are promises the shell had not kept.

test("ED-02: the mode switch paints source mode from the same document", async ({ page }) => {
  await openWithDraft(page);
  await expect(page.locator("[data-editor-surface]")).toHaveCount(1);
  await page.locator("[data-mode-switch]").first().click();

  const source = page.locator("[data-source-mode]");
  await expect(source).toHaveCount(1);
  await expect(page.locator("[data-editor-surface]")).toHaveCount(0);
  // The textarea owns the caret and the painted layer is hidden from a screen
  // reader, which is the arrangement `source-mode.ts` exists to keep.
  await expect(source.locator("textarea[data-source-editor]")).toHaveCount(1);
  await expect(source.locator("pre.highlight[aria-hidden='true']")).toHaveCount(1);
  await expect(page.locator("[data-announce]")).toContainText("Source mode");
});

test("the switch is reversible and the label says where it goes next", async ({ page }) => {
  await openWithDraft(page);
  const button = page.locator("[data-mode-switch]").first();
  await expect(button).toHaveText(/Source/);
  await button.click();
  await expect(button).toHaveText(/Visual/);
  await button.click();
  await expect(page.locator("[data-editor-surface]")).toHaveCount(1);
  await expect(page.locator("[data-source-mode]")).toHaveCount(0);
  await expect(button).toHaveText(/Source/);
});

test("ED-03(d): the switch is lossless because both modes render one document", async ({ page }) => {
  // Not a claim about this function — both modes map the same `SourceDocument`,
  // so a switch is a re-render. What is asserted is that the bytes the source
  // pane shows are the draft's own, after a round trip through visual mode.
  await openWithDraft(page);
  const button = page.locator("[data-mode-switch]").first();
  await button.click();
  const first = await page.locator("[data-source-mode] textarea[data-source-editor]").inputValue();
  await button.click();
  await button.click();
  const second = await page.locator("[data-source-mode] textarea[data-source-editor]").inputValue();
  expect(second).toBe(first);
  expect(first.length).toBeGreaterThan(40);
});

test("Control+E switches mode, the shortcut the toolbar has always advertised", async ({ page }) => {
  await openWithDraft(page);
  await page.locator("body").click();
  await page.keyboard.press("Control+e");
  await expect(page.locator("[data-source-mode]")).toHaveCount(1);
});

test("Control+E works from inside the source textarea, which is the only way back", async ({ page }) => {
  // I first wrote this asserting the opposite — that the shortcut must not fire
  // while the author is typing — by carrying over the reasoning for `?`, which
  // *is* a character and would otherwise be swallowed. Control+E is a chord: it
  // inserts nothing, and in source mode the whole pane is one textarea, so
  // guarding on "is the author typing" would make the documented shortcut dead
  // in the mode it is most needed. The code was right and the test was wrong.
  await openWithDraft(page);
  await page.locator("[data-mode-switch]").first().click();
  const editor = page.locator("[data-source-mode] textarea[data-source-editor]");
  await editor.click();
  await page.keyboard.press("Control+e");
  await expect(page.locator("[data-source-mode]")).toHaveCount(0);
  await expect(page.locator("[data-editor-surface]")).toHaveCount(1);
});

test("`?` inside a textarea types a question mark rather than opening help", async ({ page }) => {
  // The guard that does belong, and the reason Control+E does not need it.
  await openWithDraft(page);
  await page.locator("[data-mode-switch]").first().click();
  const editor = page.locator("[data-source-mode] textarea[data-source-editor]");
  await editor.click();
  const before = await editor.inputValue();
  await page.keyboard.press("?");
  await expect(page.locator("[data-panel] [data-help-topic]")).toHaveCount(0);
  expect(await editor.inputValue()).not.toBe(before);
});

test("with no draft the switch says so rather than blanking the page", async ({ page }) => {
  await page.goto(server.url);
  await page.locator("[data-mode-switch]").first().click();
  await expect(page.locator("[data-announce]")).toContainText("no draft open");
  await expect(page.locator("[data-source-mode]")).toHaveCount(0);
});

test("ED-75: the primary action is the one the author's role allows", async ({ page }) => {
  // `renderToolbar` is superseded by the shell's own header — it emits a whole
  // `<header>` and predates the guide controls, so mounting it would nest one
  // header in another and delete six working buttons. What was real in it is the
  // role-aware primary action, and that is what the mount applies.
  await openWithDraft(page);
  const primary = page.locator("[data-action]");
  await expect(primary).toHaveCount(1);
  await expect(primary).not.toHaveText("");
  // `primaryAction` returns exactly these two; an earlier version of this line
  // allowed a third that the function cannot produce, which would have passed
  // whatever the code did.
  const action = await primary.getAttribute("data-action");
  expect(["publish", "suggest"]).toContain(action);
  const label = await primary.textContent();
  expect(["Publish", "Submit for review", "Suggest an edit"]).toContain(label?.trim());
});

test("ED-74: the tour does not block the controls it is describing", async ({ page }) => {
  // It took the mode-switch tests 30s each to time out on this: the step was an
  // absolute box whose static position in the shell's grid resolved to (0, 0),
  // so a 410x226 panel sat over the toolbar and a first-time author's first
  // click hit the tour's heading. `aria-modal="false"` says the page underneath
  // stays usable, so this asserts that it does.
  await page.goto(server.url);
  const step = page.locator("[data-tour-step]");
  await expect(step).toHaveCount(1);

  const blocked = await page.evaluate(() => {
    const covered: string[] = [];
    for (const control of document.querySelectorAll("header.toolbar button")) {
      const box = control.getBoundingClientRect();
      const hit = document.elementFromPoint(box.x + box.width / 2, box.y + box.height / 2);
      if (hit !== control && !control.contains(hit)) covered.push(control.textContent?.trim() ?? "?");
    }
    return covered;
  });
  expect(blocked, "toolbar controls under the tour").toEqual([]);

  // And the step's own buttons still take clicks, or the tour cannot be used.
  await step.locator("[data-tour-next]").click();
  await expect(page.locator("[data-tour-step]")).toHaveAttribute("data-tour-step", /\S/);
  await expect(page.locator("[data-announce]")).toContainText("Tour step 2");
});

// --- the layer behind the panes: an edit has to reach the document -------------
//
// Mounting a pane makes it visible, not effective. `valueFromControl`,
// `writeFrontmatter` and `serializeModel` were all written, tested and called by
// nobody, so an author could open the form, type into it, and have the keystroke
// discarded with nothing saying so. These tests type.

test("ED-11: typing an invalid value shows the schema error and blocks the save", async ({ page }) => {
  // `facts` is the field this is driven through, and finding that out was the
  // work. Of the schema's 38 fields exactly one can produce a value the
  // validator rejects: `facts` is an `object` control, so text that is not JSON
  // falls through as a string where an object is wanted. Every `text`, `list`
  // and `boolean` control can only produce what the schema already accepts, so
  // for those the row's criterion is unreachable by construction — not untested.
  //
  // It is also advanced, so reaching it drives the third clause: the disclosure.
  await openWithDraft(page);
  await page.locator("[data-open-frontmatter]").click();
  const form = page.locator("[data-panel] [data-frontmatter]");
  await expect(form.locator("[data-field-error]")).toHaveCount(0);
  await expect(form.locator("[name='facts']")).toBeHidden();

  await form.locator("[data-advanced-frontmatter] > summary").click();
  await expect(form.locator("[name='facts']")).toBeVisible();

  await form.locator("[name='facts']").fill("not an object");
  await expect(form.locator("[data-field-error='facts']")).toHaveCount(1);
  await expect(form.locator("[data-field-error='facts']")).toContainText("expects object");
  await expect(form.locator("[data-field='facts']")).toHaveAttribute("data-invalid", "true");
  await expect(form.locator("[data-action='save']")).toBeDisabled();
  await expect(page.locator("#frontmatter-save-reason")).toContainText(/to fix before this can be saved/);
});

test("the advanced disclosure stays open while an advanced field is being typed in", async ({ page }) => {
  // It did not. The disclosure's open state is DOM state and the repaint rebuilt
  // the form closed, so the second keystroke in any advanced field landed on a
  // hidden control — every advanced field is inside that `<details>`.
  await openWithDraft(page);
  await page.locator("[data-open-frontmatter]").click();
  const form = page.locator("[data-panel] [data-frontmatter]");
  await form.locator("[data-advanced-frontmatter] > summary").click();
  const facts = form.locator("[name='facts']");
  await facts.fill("x");
  await expect(form.locator("[data-advanced-frontmatter]")).toHaveAttribute("open", "");
  await facts.fill("xy");
  await expect(facts).toBeVisible();
  await expect(facts).toBeFocused();
});

test("a valid value clears the error and unblocks the save", async ({ page }) => {
  await openWithDraft(page);
  await page.locator("[data-open-frontmatter]").click();
  const form = page.locator("[data-panel] [data-frontmatter]");
  await form.locator("[data-advanced-frontmatter] > summary").click();
  await form.locator("[name='facts']").fill("not an object");
  await expect(form.locator("[data-action='save']")).toBeDisabled();
  await form.locator("[name='facts']").fill('{ "price": 9 }');
  await expect(form.locator("[data-field-error='facts']")).toHaveCount(0);
  await expect(form.locator("[data-action='save']")).toBeEnabled();
});

test("the form says which fields it does not check, because it checks types only", async ({ page }) => {
  // The limit, asserted rather than left implicit. `validateFrontmatter` checks
  // types; it does not check patterns, so a malformed ULID in `id` passes this
  // form and fails the build. That is why ED-11 is `partial` and not
  // `implemented`, and why the live region names a count instead of staying
  // silent — silence here would read as "all checked".
  await openWithDraft(page);
  await page.locator("[data-open-frontmatter]").click();
  const form = page.locator("[data-panel] [data-frontmatter]");
  await form.locator("[data-advanced-frontmatter] > summary").click();

  await form.locator("[name='id']").fill("plainly-not-a-ulid");
  await expect(form.locator("[data-field-error='id']")).toHaveCount(0);
  await expect(page.locator("#frontmatter-save-reason")).toContainText(
    /fields? the build checks rather than this form/,
  );
});

test("the edit reaches the document, and only the line it changed", async ({ page }) => {
  // `writeFrontmatter` is byte-preserving for every untouched line, and the
  // round trip through `serializeModel` is what puts the change in the source.
  // Read it back out of source mode rather than trusting the form.
  await openWithDraft(page);
  await page.locator("[data-open-frontmatter]").click();
  await page.locator("[data-panel] [data-frontmatter] [name='title']").fill("A changed title");
  await page.keyboard.press("Escape");
  await page.locator("[data-mode-switch]").first().click();

  const source = await page.locator("[data-source-mode] textarea[data-source-editor]").inputValue();
  expect(source).toContain("A changed title");
  expect(source).not.toContain("title: Limits");
  // Every other front matter line survives untouched.
  expect(source).toMatch(/^---\n/);
  expect(source.split("\n").filter((line) => line.startsWith("description:")).length).toBe(1);
});

test("typing does not move the caret to another field", async ({ page }) => {
  // Showing a new validation result means re-rendering conditional markup, and
  // re-rendering under a typing author is how a form becomes unusable. The
  // restore is by control name and `selectionStart`.
  await openWithDraft(page);
  await page.locator("[data-open-frontmatter]").click();
  const title = page.locator("[data-panel] [data-frontmatter] [name='title']");
  await title.click();
  await title.press("End");
  await title.pressSequentially("XY");
  await expect(title).toBeFocused();
  const caret = await title.evaluate((node) => (node as HTMLInputElement).selectionStart);
  const value = await title.inputValue();
  expect(caret).toBe(value.length);
  expect(value).toMatch(/XY$/);
});

test("ED-21: Save says the route is not built rather than looking like it worked", async ({ page }) => {
  // `drafts.save` is `servedBy: "unbuilt"`. A button that silently does nothing
  // teaches an author that their edit was taken.
  await openWithDraft(page);
  await page.locator("[data-open-frontmatter]").click();
  await page.locator("[data-panel] [data-frontmatter] [name='title']").fill("Another title");
  await page.locator("[data-panel] [data-action='save']").click();
  await expect(page.locator("[data-announce]")).toContainText("not built yet");
  await expect(page.locator("[data-announce]")).toContainText("Nothing has been written");
});
