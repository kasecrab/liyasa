// ED-80's acceptance test: axe and a keyboard-only pass over the editor.
//
// > Given the editor and dashboard; when axe and the keyboard-only script run;
// > then zero violations and every action is reachable.
//
// The page is loaded from the filesystem rather than from a server. The editor
// is a static shell plus one classic script, and nothing it needs at load time
// comes over HTTP — `/_liyasa/editor/` is WP-14's surface and does not exist
// in any build yet (`web/editor/src/api.ts` says so per route). Pointing at a
// server would test the server's absence, which is not what ED-80 is about.

import { readdirSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

import AxeBuilder from "@axe-core/playwright";
import { expect, test } from "@playwright/test";

const HERE = dirname(fileURLToPath(import.meta.url));
const EDITOR = pathToFileURL(resolve(HERE, "../../editor/index.html")).href;

test.beforeEach(async ({ page }) => {
  await page.goto(EDITOR);
});

test("axe finds no violation in the editor shell", async ({ page }) => {
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
  const controls = await page.locator("button, a[href], input, select, textarea").count();
  expect(controls).toBeGreaterThan(0);

  const reached = new Set<string>();
  for (let step = 0; step < controls * 3; step += 1) {
    await page.keyboard.press("Tab");
    const marker = await page.evaluate(() => {
      const active = document.activeElement;
      if (!active || active === document.body) return null;
      return (
        active.getAttribute("data-action") ??
        active.getAttribute("data-mode-switch") ??
        active.getAttribute("data-help") ??
        active.textContent?.trim() ??
        active.tagName
      );
    });
    if (marker) reached.add(marker);
    if (reached.size >= controls) break;
  }
  expect(reached.size).toBeGreaterThanOrEqual(controls);
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
  await page.goto(EDITOR);
  const duration = await page.evaluate(() => {
    const probe = document.createElement("div");
    probe.className = "transition";
    document.body.append(probe);
    const value = getComputedStyle(probe).transitionDuration;
    probe.remove();
    return value;
  });
  expect(duration.startsWith("0s") || duration.startsWith("0.00")).toBe(true);
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
