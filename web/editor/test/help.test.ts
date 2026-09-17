// ED-32, ED-72, ED-74 and ED-80: the feed, the vocabulary, the templates, and
// the accessibility decisions a test can check without a browser.

import test from "node:test";
import assert from "node:assert/strict";

import { ACTIVITY_KINDS, KIND_LABEL, byDay, counts, feed } from "../src/activity.ts";
import { HELP, TEMPLATES, TOUR, VOCABULARY, findTemplate, pageFromTemplate, say } from "../src/help.ts";
import {
  ACTIONS,
  LANDMARKS,
  SHORTCUTS,
  actionsWithoutShortcut,
  chartTable,
  focusVisible,
  motionDuration,
  reviewAnnouncement,
  saveAnnouncement,
  shortcutCollisions,
  validationAnnouncement,
} from "../src/a11y.ts";

const DAY = 86_400_000;
const NOW = Date.parse("2026-09-18T12:00:00Z");

function entry(id: string, kind: (typeof ACTIVITY_KINDS)[number], at: number, actor = "ada", pages = ["a.md"]) {
  return { id, kind, actor, subject: id, summary: `${kind} ${id}`, at, pages };
}

test("ED-32: the feed carries all five kinds the requirement names", () => {
  assert.deepEqual(ACTIVITY_KINDS, ["draft", "review", "publish", "proposal", "drift"]);
  for (const kind of ACTIVITY_KINDS) assert.ok(KIND_LABEL[kind], `${kind} has a label`);
});

test("ED-32: the feed is newest first and filters by kind, actor and page", () => {
  const entries = [
    entry("e1", "draft", NOW - 3 * DAY),
    entry("e2", "publish", NOW - 1 * DAY, "bob"),
    entry("e3", "drift", NOW, "ada", ["b.md"]),
  ];
  assert.deepEqual(feed(entries).map((row) => row.id), ["e3", "e2", "e1"]);
  assert.deepEqual(feed(entries, { kinds: ["publish"] }).map((row) => row.id), ["e2"]);
  assert.deepEqual(feed(entries, { actor: "bob" }).map((row) => row.id), ["e2"]);
  assert.deepEqual(feed(entries, { page: "b.md" }).map((row) => row.id), ["e3"]);
  assert.deepEqual(feed(entries, { since: NOW - 2 * DAY }).map((row) => row.id), ["e3", "e2"]);
});

test("ED-32: two entries at the same instant are still in a stable order", () => {
  // An unstable sort makes the feed shuffle on every poll, which reads as
  // activity that did not happen.
  const entries = [entry("b", "draft", NOW), entry("a", "draft", NOW)];
  assert.deepEqual(feed(entries).map((row) => row.id), ["a", "b"]);
});

test("ED-32: days group in UTC, so two people see the same groups", () => {
  const groups = byDay([entry("e1", "draft", NOW), entry("e2", "draft", NOW - DAY), entry("e3", "draft", NOW - 5 * DAY)], NOW);
  assert.deepEqual(groups.map((group) => group.label), ["Today", "Yesterday", "2026-09-13"]);
  assert.equal(groups[0]?.entries.length, 1);
});

test("ED-32: the chip counts are of everything, not of what is shown", () => {
  const entries = [entry("e1", "draft", NOW), entry("e2", "draft", NOW), entry("e3", "drift", NOW)];
  assert.deepEqual(counts(entries), { draft: 2, review: 0, publish: 0, proposal: 0, drift: 1 });
});

test("ED-72: the five words the requirement names are all in the vocabulary", () => {
  for (const word of ["draft", "suggest", "review", "publish", "undo"]) {
    assert.ok(VOCABULARY[word], `${word} is in the vocabulary`);
    assert.ok(VOCABULARY[word]?.git, `${word} names the git term behind it`);
    assert.match(VOCABULARY[word]?.explains ?? "", /\S/);
  }
});

test("ED-72: the git term appears only under the advanced disclosure", () => {
  assert.equal(say("draft", false), "draft");
  assert.equal(say("draft", true), "draft (branch)");
  assert.equal(say("publish", false), "publish");
  assert.equal(say("publish", true), "publish (merge and deploy)");
});

test("a word the vocabulary does not hold is passed through unchanged", () => {
  assert.equal(say("kumquat", true), "kumquat");
});

test("ED-74: the tour points at things that exist on the page", () => {
  assert.ok(TOUR.length >= 5, "a tour worth taking");
  for (const step of TOUR) {
    assert.match(step.target, /^\[data-[a-z-]+\]$/, `${step.id} targets a data attribute`);
    assert.ok(step.body.length > 40, `${step.id} says something`);
  }
  assert.equal(new Set(TOUR.map((step) => step.id)).size, TOUR.length, "no two steps share an id");
});

test("ED-74: there is a template for each documentation type", () => {
  const kinds = new Set(TEMPLATES.map((template) => template.kind));
  for (const kind of ["tutorial", "how-to", "reference", "explanation"]) {
    assert.ok(kinds.has(kind as never), `${kind} has a template`);
  }
});

test("ED-74: a template is filled with guidance, not with lorem ipsum", () => {
  // Somebody who has never written documentation needs to see what a good
  // section looks like; an empty page with a heading teaches nothing.
  for (const template of TEMPLATES) {
    assert.ok(template.body.includes("## "), `${template.id} has sections`);
    assert.ok(template.body.length > 150, `${template.id} has guidance in it`);
    assert.ok(!/lorem|ipsum|TODO|FIXME/i.test(template.body), `${template.id} has no placeholder text`);
  }
});

test("ED-74: a page from a template carries its own id and title", () => {
  const page = pageFromTemplate("how-to", { title: "Raise a rate limit", pageId: "01ABCDEFGHJKMNPQRSTVWXYZ00" });
  assert.match(page, /^---\nid: 01ABCDEFGHJKMNPQRSTVWXYZ00\ntitle: Raise a rate limit\n---\n\n/);
  assert.ok(page.includes(findTemplate("how-to")?.body ?? "!"));
});

test("a template that does not exist is refused", () => {
  assert.throws(() => pageFromTemplate("nonsense", { title: "x", pageId: "y" }), /no page template/);
});

test("ED-74: contextual help covers the panes that need explaining", () => {
  for (const topic of ["frontmatter", "templating", "components", "review"]) {
    assert.ok(HELP[topic]?.body.length ?? 0 > 20, `${topic} has help`);
  }
});

test("ED-80: autosave is announced politely and a failure assertively", () => {
  // Interrupting somebody mid-sentence every few seconds to say "saved" makes
  // the editor unusable with a screen reader; losing work silently is worse.
  assert.equal(saveAnnouncement("saved").politeness, "polite");
  assert.equal(saveAnnouncement("saving").politeness, "polite");
  assert.equal(saveAnnouncement("failed").politeness, "assertive");
  assert.equal(saveAnnouncement("stale").politeness, "assertive");
  assert.match(saveAnnouncement("failed").message, /still here/);
});

test("ED-80: validation announces counts, and only errors interrupt", () => {
  assert.equal(validationAnnouncement(0, 0).message, "No problems found");
  assert.equal(validationAnnouncement(0, 3).politeness, "polite");
  assert.equal(validationAnnouncement(2, 0).politeness, "assertive");
  assert.match(validationAnnouncement(1, 1).message, /1 problem and 1 suggestion/);
  assert.match(validationAnnouncement(1, 0).message, /F8/);
});

test("ED-80: review state is announced", () => {
  assert.match(reviewAnnouncement("approved").message, /Approved/);
  assert.match(reviewAnnouncement("changes-requested").message, /Changes requested/);
});

test("ED-80: every action has a keyboard route", () => {
  assert.deepEqual(actionsWithoutShortcut(ACTIONS), []);
  assert.ok(ACTIONS.includes("open-properties"), "the properties form is reachable");
  assert.ok(ACTIONS.includes("next-diff-file"), "the review diff is reachable");
  assert.ok(ACTIONS.includes("chart-as-table"), "a chart has a data-table alternative");
});

test("ED-80: an action with no shortcut is reported rather than assumed reachable", () => {
  // The check has to be able to fail, or "full keyboard operation" is a claim
  // nothing tests.
  assert.deepEqual(actionsWithoutShortcut([...ACTIONS, "invented-action"]), ["invented-action"]);
});

test("ED-80: no two shortcuts collide within a scope", () => {
  assert.deepEqual(shortcutCollisions(), []);
  assert.ok(SHORTCUTS.length > 15);
});

test("ED-80: reduced motion means no motion, not less", () => {
  assert.equal(motionDuration(200, false), 200);
  assert.equal(motionDuration(200, true), 0);
});

test("ED-80: focus is always visible", () => {
  assert.equal(focusVisible(), true);
});

test("ED-80: the landmarks a screen reader jumps between are all labelled", () => {
  for (const landmark of LANDMARKS) {
    assert.ok(landmark.label.length > 2, `${landmark.role} has a label`);
  }
  assert.ok(LANDMARKS.some((landmark) => landmark.role === "main"));
});

test("ED-80: a chart's table is built from the series the chart draws", () => {
  const table = chartTable([
    { label: "Views", points: [{ x: "Mon", y: 10 }, { x: "Tue", y: 20 }] },
    { label: "Edits", points: [{ x: "Tue", y: 3 }] },
  ]);
  assert.deepEqual(table.columns, ["", "Views", "Edits"]);
  assert.deepEqual(table.rows, [
    ["Mon", 10, 0],
    ["Tue", 20, 3],
  ]);
});
