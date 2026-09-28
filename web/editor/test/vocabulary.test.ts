// ED-72: the vocabulary is plain "throughout", which was a judgement about my
// own prose until this file. It renders every pane and fails on a git word
// outside the advanced disclosure.
//
// This started life as a probe run outside the tree while a gate held it
// frozen, and its first run reported `component-head` as the git ref `HEAD`.
// The case-sensitive second pass below is that finding.
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

import { buildModel } from "../src/model.ts";
import { formFields, validateFrontmatter } from "../src/frontmatter.ts";
import { feed } from "../src/activity.ts";
import { updateANumber } from "../src/tasks.ts";
import { renderSurface, unexpanded } from "../src/view/blocks.ts";
import { renderFrontmatterForm } from "../src/view/form.ts";
import { renderProperties } from "../src/view/properties.ts";
import { renderSuggestions } from "../src/view/suggestions.ts";
import { renderActivity, renderConflict, renderDrafts, renderMedia } from "../src/view/panes.ts";
import { renderContextToolbar, renderHelp, renderProposal, renderTaskForm, renderTaskList, renderTemplatePicker, renderTourStep, renderVocabulary } from "../src/view/guides.ts";

// Case-insensitive for the words that are only ever git words, and a separate
// case-sensitive pass for `HEAD`: `component-head` is a CSS class and the first
// version of this sweep reported it, which is the shape of a check that fails on
// its own vocabulary rather than on the product's.
const HERE = dirname(fileURLToPath(import.meta.url));

const GIT = /\b(branch|branches|commit|commits|committed|pull request|merge|merged|merging|rebase|repository)\b/gi;
const GIT_REF = /\bHEAD\b/g;

const SCHEMA = JSON.parse(readFileSync(resolve(HERE, "../../../schemas/frontmatter.json"), "utf8"));
const CORPUS = JSON.parse(readFileSync(resolve(HERE, "fixtures/segments.json"), "utf8"));

function strip(markup: string): string {
  // The disclosure is where git terms belong, so it is cut out before the sweep.
  return markup
    .replace(/<details class="vocabulary"[\s\S]*?<\/details>/g, "")
    .replace(/<dl class="help-terms"[\s\S]*?<\/dl>/g, "");
}

function panes(): { name: string; markup: string }[] {
  const page = CORPUS.find((each: { path: string }) => each.path === "limits.md") ?? CORPUS[0];
  const model = buildModel(page.document, page.source);
  const values = { title: "Limits", draft: true };
  const drafts = [{ id: "liyasa/ana/limits", title: "Raise the cap", author: "ana", status: "in-review" as const, updatedAt: 0, pages: ["a.md"] }];
  return [
    { name: "block editor", markup: String(renderSurface(model, unexpanded())) },
    { name: "front matter form", markup: String(renderFrontmatterForm({ fields: formFields(SCHEMA), values, validation: validateFrontmatter(SCHEMA, values), advanced: true })) },
    { name: "properties", markup: String(renderProperties({ component: "note", props: {}, block: "0" })) },
    { name: "suggestions", markup: String(renderSuggestions({ id: "r", operation: "tighten" as const, suggestions: [{ id: "s", target: "0", before: "a\n", after: "b\n", rationale: "why", status: "pending" as const, diagnostics: [] }], withheld: [] })) },
    { name: "drafts", markup: String(renderDrafts({ drafts, source: "served", now: 86_400_000 })) },
    { name: "conflict", markup: String(renderConflict({ path: "a.md", conflicts: [{ line: 1, base: "o\n", mine: "m\n", theirs: "t\n" }], rendered: { mine: "<p>m</p>", theirs: "<p>t</p>" } })) },
    { name: "activity", markup: String(renderActivity({ entries: feed([{ id: "a", kind: "publish", actor: "ana", summary: "published", at: 0 }]), source: "served", now: 86_400_000 })) },
    { name: "media", markup: String(renderMedia({ assets: [{ path: "a.png", alt: "a", bytes: 10, usedOn: [] }], source: "served" })) },
    { name: "context toolbar", markup: String(renderContextToolbar({ readerGroups: [] }, { versions: ["1"], locales: ["en"], readerGroups: ["staff"] })) },
    { name: "vocabulary", markup: String(renderVocabulary(true)) },
    { name: "tour", markup: String(renderTourStep(0)) },
    { name: "help", markup: String(renderHelp("frontmatter", true)) },
    { name: "templates", markup: String(renderTemplatePicker()) },
    { name: "task list", markup: String(renderTaskList()) },
    { name: "task form", markup: String(renderTaskForm("update-a-number")) },
    { name: "proposal", markup: String(renderProposal(updateANumber([], { fact: "p", value: "1", factFile: "f.json", factSource: '{ "p": 0 }' }))) },
  ];
}

test("ED-72: no pane says a git word outside the advanced disclosure", () => {
  const leaks: string[] = [];
  for (const pane of panes()) {
    const clean = strip(pane.markup);
    for (const hit of clean.matchAll(GIT)) leaks.push(`${pane.name}: ${hit[0]}`);
    for (const hit of clean.matchAll(GIT_REF)) leaks.push(`${pane.name}: ${hit[0]}`);
  }
  assert.deepEqual([...new Set(leaks)], []);
});

test("the sweep can see a git word when there is one", () => {
  // The positive control: without it a passing sweep proves only that the
  // regular expression never matches anything.
  assert.notDeepEqual([...strip("<p>on your branch</p>").matchAll(GIT)], []);
  assert.notDeepEqual([...strip("<p>at HEAD</p>").matchAll(GIT_REF)], []);
  assert.deepEqual([...strip('<div class="component-head">x</div>').matchAll(GIT_REF)], [], "a CSS class is not a git ref");
  assert.deepEqual([...strip(String(renderVocabulary(true))).matchAll(GIT)], [], "the disclosure is cut out");
});
