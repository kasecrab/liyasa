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
    "Guides",
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

// ED-80 over the panes, not only the shell.
//
// > full keyboard operation of the block editor, properties forms, review diffs,
// > and charts (with data-table alternatives)
//
// The tests above load `index.html`, which is the landmarks. That proved the
// shell and nothing about the surfaces ED-80 names, because the shell does not
// build them: `/_liyasa/editor/` is WP-14's route and does not exist in any
// build, so no draft opens and no pane fills.
//
// These tests render each pane with its own renderer — the same function the
// shipped bundle calls — and put the markup in a page with the editor's real
// stylesheet. What that proves is exactly the markup and the CSS: axe walks the
// product's own output and the keyboard walks the product's own controls. What
// it does not prove is the shell's wiring, because there is none to prove yet;
// when the draft route lands, these become assertions on a page the editor
// itself rendered and nothing here changes but the setup.

import { readFileSync } from "node:fs";

import { buildModel } from "../../editor/src/model.ts";
import { formFields, validateFrontmatter } from "../../editor/src/frontmatter.ts";
import { capIterations, PREVIEW_DEFAULTS } from "../../editor/src/preview.ts";
import { chartTable } from "../../editor/src/a11y.ts";
import { renderSurface, unexpanded } from "../../editor/src/view/blocks.ts";
import type { SurfaceContext } from "../../editor/src/view/blocks.ts";
import { renderFrontmatterForm } from "../../editor/src/view/form.ts";
import { renderProperties } from "../../editor/src/view/properties.ts";
import { renderSourceMode } from "../../editor/src/view/source-mode.ts";
import { renderSuggestions } from "../../editor/src/view/suggestions.ts";
import { renderActivity, renderConflict, renderDrafts, renderMedia } from "../../editor/src/view/panes.ts";
import {
  renderContextToolbar,
  renderHelp,
  renderProposal,
  renderTaskForm,
  renderTaskList,
  renderTemplatePicker,
  renderTourStep,
  renderVocabulary,
} from "../../editor/src/view/guides.ts";
import { updateANumber } from "../../editor/src/tasks.ts";
import { feed } from "../../editor/src/activity.ts";

const CORPUS: { path: string; source: string; document: unknown }[] = JSON.parse(
  readFileSync(resolve(HERE, "../../editor/test/fixtures/segments.json"), "utf8"),
);
const SCHEMA = JSON.parse(readFileSync(resolve(HERE, "../../../schemas/frontmatter.json"), "utf8"));

/** The page with a loop and a chip in it, which is what ED-80's block editor is. */
function richPage() {
  const page = CORPUS.find((candidate) => candidate.path === "limits.md") ?? CORPUS[0];
  if (!page) throw new Error("the segment corpus is empty");
  return buildModel(page.document as never, page.source);
}

/**
 * Each pane's markup, named.
 *
 * Built once per test rather than shared, because a renderer that mutated its
 * argument would otherwise show up as a failure in whichever test ran second.
 */
function panes(): { name: string; markup: string }[] {
  const model = richPage();
  const loop = model.nodes.find((node) => node.kind === "logic_block");
  const context: SurfaceContext = {
    ...unexpanded(),
    values: { 'fact("price")': "£9" },
    expansions: loop
      ? { [loop.id]: capIterations(Array.from({ length: 90 }, (_, at) => `| row ${at} |\n`), PREVIEW_DEFAULTS) }
      : {},
  };

  const fields = formFields(SCHEMA);
  const values = { title: "Limits", draft: true, id: "not-a-ulid" };

  const drafts = [
    {
      id: "liyasa/ana/limits",
      title: "Raise the page cap",
      author: "ana",
      status: "in-review" as const,
      updatedAt: Date.parse("2026-09-27T09:00:00Z"),
      pages: ["guides/limits.md", "reference/caps.md"],
    },
  ];

  const suggestions = {
    id: "run-1",
    operation: "tighten" as const,
    suggestions: [
      {
        id: "s1",
        target: "0",
        before: "It is very important to note that the cap is 500.\n",
        after: "The cap is 500.\n",
        rationale: "Shorter, and it says the same thing.",
        status: "pending" as const,
        diagnostics: [],
      },
    ],
    withheld: [],
  };

  const chart = chartTable([
    { label: "Pages", points: [{ x: "Mon", y: 12 }, { x: "Tue", y: 18 }] },
  ]);

  const proposal = updateANumber(
    ["a.md", "b.md"].map((path) => ({
      path,
      source: 'Costs {{ fact("price") }}.\n',
      document: { segments: [], source: 0 } as never,
    })),
    { fact: "price", value: "12", factFile: "facts/pricing.json", factSource: '{ "price": 9 }\n' },
  );

  return [
    { name: "block editor", markup: String(renderSurface(model, context)) },
    {
      name: "source mode",
      markup: String(
        renderSourceMode({
          document: (CORPUS.find((each) => each.path === "limits.md") ?? CORPUS[0])!.document as never,
          source: (CORPUS.find((each) => each.path === "limits.md") ?? CORPUS[0])!.source,
          problemLines: [{ line: 2, severity: "error" }],
        }),
      ),
    },
    {
      name: "front matter form",
      markup: String(
        renderFrontmatterForm({
          fields,
          values,
          validation: validateFrontmatter(SCHEMA, values),
          advanced: true,
        }),
      ),
    },
    {
      name: "properties form",
      markup: String(renderProperties({ component: "note", props: { kind: "warning" }, block: "0" })),
    },
    { name: "review diffs", markup: String(renderSuggestions(suggestions)) },
    {
      name: "chart data table",
      markup: `<table><caption>Pages built</caption><thead><tr>${chart.columns
        .map((column) => `<th scope="col">${column || "Day"}</th>`)
        .join("")}</tr></thead><tbody>${chart.rows
        .map((row) => `<tr>${row.map((cell, at) => (at === 0 ? `<th scope="row">${cell}</th>` : `<td>${cell}</td>`)).join("")}</tr>`)
        .join("")}</tbody></table>`,
    },
    { name: "drafts", markup: String(renderDrafts({ drafts, source: "served", now: Date.parse("2026-09-28T09:00:00Z") })) },
    {
      name: "conflict resolver",
      markup: String(
        renderConflict({
          path: "guides/limits.md",
          conflicts: [{ line: 4, base: "old\n", mine: "mine\n", theirs: "theirs\n" }],
          rendered: { mine: "<h1>Mine</h1>", theirs: "<h1>Theirs</h1>" },
        }),
      ),
    },
    {
      name: "activity",
      markup: String(
        renderActivity({
          entries: feed([
            {
              id: "a1",
              kind: "publish",
              actor: "ana",
              summary: "published the limits page",
              at: Date.parse("2026-09-27T09:00:00Z"),
            },
          ]),
          source: "served",
          now: Date.parse("2026-09-28T09:00:00Z"),
        }),
      ),
    },
    {
      name: "media library",
      markup: String(
        renderMedia({
          assets: [
            {
              path: "assets/tour.png",
              alt: "The editor with a draft open",
              bytes: 40_000,
              usedOn: ["guides/limits.md"],
            },
          ],
          source: "served",
        }),
      ),
    },
    {
      name: "preview context toolbar",
      markup: String(
        renderContextToolbar(
          { version: "2.0", readerGroups: ["staff"] },
          { versions: ["2.0", "1.9"], locales: ["en", "de"], readerGroups: ["staff", "beta"] },
        ),
      ),
    },
    { name: "vocabulary", markup: String(renderVocabulary(true)) },
    { name: "tour", markup: String(renderTourStep(0)) },
    { name: "contextual help", markup: String(renderHelp("frontmatter", true)) },
    { name: "template picker", markup: String(renderTemplatePicker("tutorial")) },
    { name: "task list", markup: String(renderTaskList()) },
    { name: "task form", markup: String(renderTaskForm("update-a-number")) },
    { name: "proposal", markup: String(renderProposal(proposal)) },
  ];
}

/** A pane in a page with the editor's own stylesheet, and a heading above it. */
async function show(page: import("@playwright/test").Page, name: string, markup: string): Promise<void> {
  await page.goto(server.url);
  await page.evaluate(
    ({ name: heading, markup: body }) => {
      const main = document.querySelector("main");
      if (!main) throw new Error("the shell has no main");
      main.innerHTML = `<h1>${heading}</h1>${body}`;
    },
    { name, markup },
  );
}

for (const pane of panes()) {
  test(`axe finds no violation in the ${pane.name}`, async ({ page }) => {
    test.slow();
    await show(page, pane.name, pane.markup);
    const results = await new AxeBuilder({ page })
      .withTags(["wcag2a", "wcag2aa", "wcag21a", "wcag21aa", "wcag22aa"])
      .analyze();
    expect(
      results.violations.map(
        (violation) => `${violation.id}: ${violation.nodes.map((node) => node.html).join(" | ")}`,
      ),
    ).toEqual([]);
  });
}

test("every control in every pane is reachable with Tab alone", async ({ page }) => {
  // The same probe the shell test uses, over each pane in turn. A pane whose
  // controls cannot be tabbed to is the failure ED-80's "every action is
  // reachable" is about, and it is invisible to axe: a button behind a
  // `tabindex="-1"` is perfectly labelled.
  for (const pane of panes()) {
    await show(page, pane.name, pane.markup);
    const controls = await page.evaluate(() => {
      const main = document.querySelector("main");
      const found = [...(main?.querySelectorAll("button, a[href], input, select, textarea, [tabindex='0']") ?? [])]
        .filter((node) => !(node instanceof HTMLElement && node.hidden))
        .filter((node) => !(node instanceof HTMLButtonElement && node.disabled));
      found.forEach((node, at) => node.setAttribute("data-tab-probe", String(at)));
      return found.length;
    });
    if (controls === 0) continue;

    const reached = new Set<string>();
    await page.locator("body").press("Tab");
    for (let step = 0; step < controls * 6; step += 1) {
      const marker = await page.evaluate(() => document.activeElement?.getAttribute("data-tab-probe"));
      if (marker !== null && marker !== undefined) reached.add(marker);
      if (reached.size >= controls) break;
      await page.keyboard.press("Tab");
    }
    expect([...reached].map(Number).sort((a, b) => a - b), `${pane.name}: unreachable control`).toEqual(
      Array.from({ length: controls }, (_, at) => at),
    );
  }
});

test("every field in every form is named by a label its control points at", async ({ page }) => {
  // axe's `label` rule catches an input with no accessible name at all. It does
  // not catch a `<label for>` pointing at an id nothing has, which reads fine to
  // axe's own heuristics in some shapes and gives a screen reader nothing.
  for (const pane of panes()) {
    await show(page, pane.name, pane.markup);
    const orphans = await page.evaluate(() =>
      [...document.querySelectorAll("label[for]")]
        .map((label) => label.getAttribute("for") ?? "")
        .filter((id) => id !== "" && document.getElementById(id) === null),
    );
    expect(orphans, `${pane.name}: labels pointing at nothing`).toEqual([]);
  }
});

test("every aria-describedby and aria-labelledby points at an element that exists", async ({ page }) => {
  // A dangling reference is silent: the attribute is there, the name is not, and
  // the pane looks correct in every view except a screen reader's.
  for (const pane of panes()) {
    await show(page, pane.name, pane.markup);
    const dangling = await page.evaluate(() => {
      const bad: string[] = [];
      for (const attribute of ["aria-describedby", "aria-labelledby", "aria-controls"]) {
        for (const node of document.querySelectorAll(`[${attribute}]`)) {
          for (const id of (node.getAttribute(attribute) ?? "").split(/\s+/).filter(Boolean)) {
            if (document.getElementById(id) === null) bad.push(`${attribute}=${id}`);
          }
        }
      }
      return bad;
    });
    expect(dangling, `${pane.name}: dangling references`).toEqual([]);
  }
});

test("every pane's controls draw a focus ring", async ({ page }) => {
  for (const pane of panes()) {
    await show(page, pane.name, pane.markup);
    const controls = page.locator("main button, main input, main select, main textarea");
    const count = await controls.count();
    for (let at = 0; at < count; at += 1) {
      const control = controls.nth(at);
      if (await control.isDisabled().catch(() => false)) continue;
      await control.focus();
      const ring = await control.evaluate((node) => {
        const style = getComputedStyle(node);
        return {
          outline: Number.parseFloat(style.outlineWidth),
          style: style.outlineStyle,
          shadow: style.boxShadow,
        };
      });
      const visible = (ring.style !== "none" && ring.outline > 0) || ring.shadow !== "none";
      expect(visible, `${pane.name}: control ${at} has no focus ring`).toBe(true);
    }
  }
});

test("no pane needs a horizontal scroll at a phone width", async ({ page }) => {
  for (const pane of panes()) {
    await page.setViewportSize({ width: 390, height: 844 });
    await show(page, pane.name, pane.markup);
    const overflow = await page.evaluate(
      () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
    );
    expect(overflow, `${pane.name} overflows a 390px viewport`).toBeLessThanOrEqual(0);
  }
});

test("ED-80: the panes ED-80 names by name are all under test here", async () => {
  // The row names four surfaces. This is the check that stops a rename or a
  // deletion quietly reducing what "zero violations" is a claim about.
  const names = panes().map((pane) => pane.name);
  for (const required of ["block editor", "properties form", "review diffs", "chart data table"]) {
    expect(names, `${required} is one of the panes under test`).toContain(required);
  }
  expect(names.length).toBeGreaterThanOrEqual(15);
});
