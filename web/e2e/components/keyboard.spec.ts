// CMP-10..13, CMP-30, CMP-31: the keyboard contract of the interactive
// components, driven by the theme's own runtime (`plan/rfcs/0400-component-behaviour-hooks.md`).
//
// The components render markup and the theme's modules enhance it, so neither
// half proves the behaviour alone: the golden files pin the attributes and
// this suite is what says the two fit together. Every assertion here is about
// what a reader reaches without a mouse.

import { expect, test } from "@playwright/test";

const TABS = "/components/tabs";
const CODE_GROUP = "/components/code-group";
const ACCORDION = "/components/accordion";
const EXPANDABLE = "/components/expandable";
const CALLOUT = "/components/callout";

test.describe("tabs", () => {
  test("only the selected tab is in the tab order", async ({ page }) => {
    await page.goto(TABS);
    const group = page.locator("#case-install [data-ly-tabs]");
    await expect(group).toHaveAttribute("data-ly-enhanced", "true");

    const tabs = group.getByRole("tab");
    await expect(tabs).toHaveCount(3);
    await expect(tabs.nth(0)).toHaveAttribute("tabindex", "0");
    await expect(tabs.nth(1)).toHaveAttribute("tabindex", "-1");
    await expect(tabs.nth(2)).toHaveAttribute("tabindex", "-1");
  });

  test("the arrow keys move the selection and wrap", async ({ page }) => {
    await page.goto(TABS);
    const group = page.locator("#case-install [data-ly-tabs]");
    const tabs = group.getByRole("tab");

    await tabs.nth(0).focus();
    await page.keyboard.press("ArrowRight");
    await expect(tabs.nth(1)).toBeFocused();
    await expect(tabs.nth(1)).toHaveAttribute("aria-selected", "true");
    await expect(tabs.nth(0)).toHaveAttribute("aria-selected", "false");

    await page.keyboard.press("ArrowRight");
    await page.keyboard.press("ArrowRight");
    await expect(tabs.nth(0), "the last tab wraps to the first").toBeFocused();

    await page.keyboard.press("ArrowLeft");
    await expect(tabs.nth(2), "the first tab wraps to the last").toBeFocused();
  });

  test("Home and End reach the ends of the group", async ({ page }) => {
    await page.goto(TABS);
    const tabs = page.locator("#case-install [data-ly-tabs]").getByRole("tab");

    await tabs.nth(1).focus();
    await page.keyboard.press("End");
    await expect(tabs.nth(2)).toBeFocused();
    await page.keyboard.press("Home");
    await expect(tabs.nth(0)).toBeFocused();
  });

  test("the selected tab is the only panel shown", async ({ page }) => {
    await page.goto(TABS);
    const group = page.locator("#case-install [data-ly-tabs]");
    // A hidden panel leaves the accessibility tree, so `getByRole` cannot see
    // it; the panels are addressed by class to assert what is hidden.
    const panels = group.locator(".ly-tabpanel");

    await expect(panels.nth(0)).toBeVisible();
    await expect(panels.nth(1)).toBeHidden();

    await group.getByRole("tab").nth(1).focus();
    await page.keyboard.press("ArrowRight");
    await expect(panels.nth(2)).toBeVisible();
    await expect(panels.nth(0)).toBeHidden();
  });

  test("a panel is labelled by the tab that opens it", async ({ page }) => {
    await page.goto(TABS);
    const group = page.locator("#case-install [data-ly-tabs]");
    const tab = group.getByRole("tab").nth(0);
    const panel = group.getByRole("tabpanel").nth(0);

    const id = await tab.getAttribute("id");
    expect(await panel.getAttribute("aria-labelledby")).toBe(id);
    expect(await tab.getAttribute("aria-controls")).toBe(await panel.getAttribute("id"));
  });
});

test.describe("code groups", () => {
  test("a code group is a tablist the arrow keys drive", async ({ page }) => {
    await page.goto(CODE_GROUP);
    const group = page.locator("#case-install [data-ly-tabs]");
    const tabs = group.getByRole("tab");

    await expect(group).toHaveAttribute("data-ly-enhanced", "true");
    await tabs.nth(0).focus();
    await page.keyboard.press("ArrowRight");
    await expect(tabs.nth(1)).toBeFocused();
    await expect(group.locator(".ly-tabpanel").nth(1)).toBeVisible();
  });

  test("the copy button appears only once the runtime can serve it", async ({ page }) => {
    await page.goto(CODE_GROUP);
    const copy = page.locator("#case-install [data-ly-copy]").first();
    await expect(copy).toBeVisible();

    // What it copies is the block it names, not whatever came first.
    const target = await copy.getAttribute("data-ly-copy");
    expect(target).toBeTruthy();
    await expect(page.locator(`#${target}`)).toHaveCount(1);
  });
});

test.describe("disclosures", () => {
  test("an accordion opens from the keyboard", async ({ page }) => {
    await page.goto(ACCORDION);
    const item = page.locator("#case-closed details");
    const summary = item.locator("summary");

    await expect(item).not.toHaveAttribute("open", /.*/);
    await summary.focus();
    await page.keyboard.press("Enter");
    await expect(item).toHaveAttribute("open", /.*/);

    await page.keyboard.press("Enter");
    await expect(item).not.toHaveAttribute("open", /.*/);
  });

  test("an accordion that ships open is open before any script runs", async ({ page }) => {
    await page.goto(ACCORDION);
    await expect(page.locator("#case-open details")).toHaveAttribute("open", /.*/);
  });

  test("an expandable is reachable by Tab and opens on Space", async ({ page }) => {
    await page.goto(EXPANDABLE);
    const item = page.locator('[data-liyasa="expandable"]').first();

    await item.locator("summary").focus();
    await expect(item.locator("summary")).toBeFocused();
    await page.keyboard.press("Space");
    await expect(item).toHaveAttribute("open", /.*/);
  });

  test("a collapsible callout is a disclosure and a plain one is not", async ({ page }) => {
    await page.goto(CALLOUT);
    await expect(page.locator("#case-collapsible details")).toHaveCount(1);
    await expect(page.locator("#case-custom details")).toHaveCount(0);
    await expect(page.locator("#case-custom aside")).toHaveCount(1);
  });
});

test.describe("steps", () => {
  test("steps are an ordered list, so their position is announced", async ({ page }) => {
    await page.goto("/components/steps");
    const list = page.locator('[data-liyasa="steps"]');
    await expect(list).toHaveCount(1);
    expect(await list.evaluate((node) => node.tagName)).toBe("OL");
    await expect(list.locator("li")).toHaveCount(3);
  });

  test("every step has an anchor of its own", async ({ page }) => {
    await page.goto("/components/steps");
    const ids = await page
      .locator('[data-liyasa="steps"] li')
      .evaluateAll((nodes) => nodes.map((node) => node.id));
    expect(ids.filter(Boolean)).toHaveLength(ids.length);
    expect(new Set(ids).size).toBe(ids.length);
  });
});
