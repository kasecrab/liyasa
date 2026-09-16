// CMP-01..75: axe over every component's gallery page.
//
// One spec rather than one per component. The gallery pages come from
// `liyasa_components::gallery`, so the list of pages is the list of
// components, and a failure names the page and the rule it broke — which is
// what a per-component file would have told us, without twenty-two files that
// differ only in a string.
//
// The shell around each gallery is deliberately thin (a landmark, one `h1`,
// one `h2` per case), so a violation here is a property of the component's own
// markup rather than of a page template the components do not own.

import AxeBuilder from "@axe-core/playwright";
import { expect, test } from "@playwright/test";

// Keep in step with `liyasa_components::gallery::names`.
const COMPONENTS = [
  "accordion",
  "badge",
  "callout",
  "card",
  "cards",
  "code-block",
  "code-group",
  "columns",
  "expandable",
  "file",
  "frame",
  "icon",
  "iframe",
  "image",
  "kbd",
  "note",
  "param",
  "request-example",
  "response-field",
  "steps",
  "tabs",
  "video",
];

const STANDARD = ["wcag2a", "wcag2aa", "wcag21a", "wcag21aa"];

for (const component of COMPONENTS) {
  test(`${component} has no accessibility violations`, async ({ page }) => {
    await page.goto(`/components/${component}`);
    const results = await new AxeBuilder({ page }).withTags(STANDARD).analyze();
    // The rule id and the markup that broke it, so a failure is actionable
    // without re-running with a reporter.
    const found = results.violations.map((violation) => ({
      id: violation.id,
      impact: violation.impact,
      nodes: violation.nodes.map((node) => node.html),
    }));
    expect(found).toEqual([]);
  });
}

test("every gallery page is reachable and names its component", async ({ page }) => {
  for (const component of COMPONENTS) {
    const response = await page.goto(`/components/${component}`);
    expect(response?.status(), component).toBe(200);
    await expect(page.locator("h1")).toHaveText(`${component} gallery`);
  }
});
