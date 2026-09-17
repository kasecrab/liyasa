// ED-80's acceptance test: axe and a keyboard-only pass over the editor.
//
// > Given the editor and dashboard; when axe and the keyboard-only script run;
// > then zero violations and every action is reachable.
//
// The editor is served by this suite's own static server rather than by the
// workspace's shared `webServer`, which builds the reference site. Nothing the
// editor needs at load time comes over the network anyway —
// `/_liyasa/editor/` is WP-14's surface and does not exist in any build yet,
// and `web/editor/src/api.ts` says so per route — so what is under test here
// is the shell, which is what ED-80 is about.

import { readdirSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import AxeBuilder from "@axe-core/playwright";
import { expect, test } from "@playwright/test";

import { startEditorServer } from "../editor/editor-server.ts";
import type { EditorServer } from "../editor/editor-server.ts";

const HERE = dirname(fileURLToPath(import.meta.url));

let server: EditorServer;

test.beforeAll(async () => {
  server = await startEditorServer();
});

test.afterAll(async () => {
  await server.close();
});

test.beforeEach(async ({ page }) => {
  await page.goto(server.url);
});

test("axe finds no violation in the editor shell", async ({ page }) => {
  // axe injects and walks the whole tree; on a machine running several builds
  // at once that outlasts the default timeout, and a timeout here would read
  // as a violation when it is a busy machine.
  test.slow();
  const results = await new AxeBuilder({ page })
    .withTags(["wcag2a", "wcag2aa", "wcag21a", "wcag21aa", "wcag22aa"])
    .analyze();
  expect(
    results.violations.map((violation) => `${violation.id}: ${violation.nodes.length} node(s)`),
  ).toEqual([]);
});

test("every landmark is present, labelled, and reachable", async ({ page }) => {
  for (const label of [
    "Editor toolbar",
    "Pages",
    "Page content",
    "Preview",
    "Problems",
    "Draft status",
  ]) {
    const region = page.locator(`[data-landmark="${label}"]`);
    await expect(region).toHaveCount(1);
    await expect(region).toHaveAttribute("aria-label", label);
  }
  await expect(page.locator("main")).toHaveCount(1);
});

test("the live region is in the accessibility tree", async ({ page }) => {
  // Hidden with `clip-path`, not with `display: none` — the one thing that
  // takes a live region out of the tree it has to stay in.
  const region = page.locator("[data-announce]");
  await expect(region).toHaveAttribute("aria-live", "polite");
  const display = await region.evaluate((node) => getComputedStyle(node).display);
  expect(display).not.toBe("none");
  const visibility = await region.evaluate((node) => getComputedStyle(node).visibility);
  expect(visibility).not.toBe("hidden");
});

test("the editor announces that it is ready", async ({ page }) => {
  await expect(page.locator("[data-announce]")).toHaveText("Editor ready");
});

test("every control in the shell is reachable with Tab alone", async ({ page }) => {
  // Each control is tagged with its own index first. Identifying the focused
  // element by its text or by an attribute it shares with another control
  // undercounts, and the test then passes with a control nobody can tab to.
  const controls = await page.evaluate(() => {
    const found = [...document.querySelectorAll("button, a[href], input, select, textarea")];
    found.forEach((node, at) => node.setAttribute("data-tab-probe", String(at)));
    return found.length;
  });
  expect(controls).toBeGreaterThan(0);

  const reached = new Set<string>();
  for (let step = 0; step < controls * 4; step += 1) {
    await page.keyboard.press("Tab");
    const marker = await page.evaluate(() => document.activeElement?.getAttribute("data-tab-probe"));
    if (marker !== null && marker !== undefined) reached.add(marker);
    if (reached.size >= controls) break;
  }
  expect([...reached].sort()).toEqual(
    Array.from({ length: controls }, (_, at) => String(at)).sort(),
  );
});

test("every focused control draws a focus ring", async ({ page }) => {
  // WCAG 2.4.7, and the failure that is invisible to whoever wrote the CSS
  // because they were using a mouse at the time.
  const controls = page.locator("button");
  const count = await controls.count();
  expect(count).toBeGreaterThan(0);
  for (let at = 0; at < count; at += 1) {
    const control = controls.nth(at);
    await control.focus();
    const outline = await control.evaluate((node) => {
      const style = getComputedStyle(node);
      return { width: style.outlineWidth, style: style.outlineStyle };
    });
    expect(outline.style, `control ${at} has no outline style`).not.toBe("none");
    expect(Number.parseFloat(outline.width), `control ${at} has a zero-width outline`).toBeGreaterThan(0);
  }
});

test("reduced motion removes transitions rather than shortening them", async ({ browser }) => {
  const context = await browser.newContext({ reducedMotion: "reduce" });
  const page = await context.newPage();
  await page.goto(server.url);
  const duration = await page.evaluate(() => {
    const probe = document.createElement("div");
    probe.className = "transition";
    document.body.append(probe);
    const value = getComputedStyle(probe).transitionDuration;
    probe.remove();
    return value;
  });
  // Chromium formats 0.01ms as `1e-05s`, so the check is numeric rather than
  // a prefix — the first version of this assertion failed on a stylesheet that
  // was doing exactly the right thing.
  const durations = duration.split(",").map((value) => Number.parseFloat(value));
  expect(durations.length).toBeGreaterThan(0);
  for (const seconds of durations) expect(seconds).toBeLessThan(0.001);
  await context.close();
});

test("the shell is usable at a phone width without a horizontal scrollbar", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  const overflow = await page.evaluate(
    () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
  );
  expect(overflow).toBeLessThanOrEqual(0);
});

test("the editor's suite covers every acceptance row the packet names", async () => {
  // Not a browser assertion: a guard against a spec file being lost in a
  // rename or a rebase, which is how an acceptance test quietly stops existing.
  const specs = readdirSync(resolve(HERE, "../editor")).filter((name) => name.endsWith(".spec.ts"));
  for (const row of ["ed_01", "ed_02", "ed_04", "ed_05", "ed_10", "ed_11", "ed_12", "ed_21", "ed_40", "ed_41", "ed_70", "ed_71"]) {
    expect(specs, `${row} has a spec file`).toContain(`${row}.spec.ts`);
  }
});
