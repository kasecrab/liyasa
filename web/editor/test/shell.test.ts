import test from "node:test";
import assert from "node:assert/strict";

import { TOUR } from "../src/help.ts";
import { START, actionFor, announcementFor, landmarkFor, reduce, renderPanel } from "../src/view/shell.ts";
import type { ShellState } from "../src/view/shell.ts";

const after = (state: ShellState, ...actions: Parameters<typeof reduce>[1][]): ShellState =>
  actions.reduce((each, action) => reduce(each, action), state);

test("the shell starts with nothing open", () => {
  assert.equal(START.panel.kind, "none");
  assert.equal(String(renderPanel(START)), "");
  assert.equal(landmarkFor(START.panel), null);
});

test("Help opens help for whatever the author is looking at", () => {
  const state = after({ ...START, context: "templating" }, { do: "open-help" });
  assert.deepEqual(state.panel, { kind: "help", topic: "templating" });
  assert.match(String(renderPanel(state)), /data-help-topic="templating"/);
});

test("pressing the same control again closes the panel", () => {
  // A button whose only effect is to re-render what is already open looks
  // broken, and the author presses it twice before concluding it is.
  const open = after(START, { do: "open-help" });
  assert.equal(after(open, { do: "open-help" }).panel.kind, "none");
  const tasks = after(START, { do: "open-tasks" });
  assert.equal(after(tasks, { do: "open-tasks" }).panel.kind, "none");
  const words = after(START, { do: "open-vocabulary" });
  assert.equal(after(words, { do: "open-vocabulary" }).panel.kind, "none");
});

test("help follows the context while it is open, and does not open by itself", () => {
  const open = after(START, { do: "open-help" }, { do: "context", topic: "components" });
  assert.deepEqual(open.panel, { kind: "help", topic: "components" });
  const closed = after(START, { do: "context", topic: "components" });
  assert.equal(closed.panel.kind, "none", "changing context does not open help");
  assert.equal(closed.context, "components");
});

test("the tour walks forward, back, and finishes exactly once", () => {
  let state = after(START, { do: "open-tour" });
  for (let step = 0; step < TOUR.length - 1; step += 1) {
    assert.deepEqual(state.panel, { kind: "tour", step });
    state = after(state, { do: "tour-next" });
  }
  assert.deepEqual(state.panel, { kind: "tour", step: TOUR.length - 1 });
  assert.equal(state.tourSeen, false, "not seen until it ends");
  state = after(state, { do: "tour-done" });
  assert.equal(state.panel.kind, "none");
  assert.equal(state.tourSeen, true);
});

test("next past the last step closes rather than rendering nothing", () => {
  // Reachable by a keyboard shortcut even though the last step's button says
  // Finish, and a tour that renders an empty dialog reads as a crash.
  const last = { ...START, panel: { kind: "tour" as const, step: TOUR.length - 1 } };
  const state = after(last, { do: "tour-next" });
  assert.equal(state.panel.kind, "none");
  assert.equal(state.tourSeen, true);
});

test("back from the first step stays put instead of closing", () => {
  const first = after(START, { do: "open-tour" });
  assert.deepEqual(after(first, { do: "tour-back" }).panel, { kind: "tour", step: 0 });
});

test("skipping the tour is remembered, so it does not reopen", () => {
  const skipped = after(START, { do: "open-tour" }, { do: "tour-skip" });
  assert.equal(skipped.tourSeen, true);
  assert.equal(after(skipped, { do: "first-visit", seen: false }).panel.kind, "none");
});

test("the tour opens itself on a first visit and never over an open panel", () => {
  assert.equal(after(START, { do: "first-visit", seen: false }).panel.kind, "tour");
  assert.equal(after(START, { do: "first-visit", seen: true }).panel.kind, "none");
  const busy = after(START, { do: "open-tasks" }, { do: "first-visit", seen: false });
  assert.equal(busy.panel.kind, "tasks", "an introduction must not interrupt");
});

test("a task runs list, form, cancel, list", () => {
  const listed = after(START, { do: "open-tasks" });
  assert.match(String(renderPanel(listed)), /data-choose-task="update-a-number"/);
  const form = after(listed, { do: "choose-task", id: "update-a-number" });
  assert.deepEqual(form.panel, { kind: "task", id: "update-a-number" });
  assert.match(String(renderPanel(form)), /data-task-form="update-a-number"/);
  assert.equal(after(form, { do: "cancel-task" }).panel.kind, "tasks");
});

test("choosing a template marks it current without closing the picker", () => {
  const chosen = after(START, { do: "open-templates" }, { do: "choose-template", id: "how-to" });
  assert.deepEqual(chosen.panel, { kind: "templates", chosen: "how-to" });
  assert.match(String(renderPanel(chosen)), /data-template="how-to" data-kind="how-to" aria-current="true"/);
});

test("advanced is the shell's, not a panel's, and survives opening one", () => {
  const advanced = after(START, { do: "toggle-advanced" });
  assert.equal(advanced.advanced, true);
  const help = after(advanced, { do: "open-help", topic: "review" });
  assert.equal(help.advanced, true);
  assert.match(String(renderPanel(help)), /data-help-terms/, "the git terms show");
  assert.ok(!String(renderPanel(after(START, { do: "open-help", topic: "review" }))).includes("data-help-terms"));
});

test("every transition that changes the panel says where focus goes", () => {
  // A panel that opens without moving focus is a panel a keyboard user has to
  // find; one that closes without giving focus back loses them their place.
  const actions: Parameters<typeof reduce>[1][] = [
    { do: "open-help" },
    { do: "open-tour" },
    { do: "open-templates" },
    { do: "open-tasks" },
    { do: "open-vocabulary" },
    { do: "close" },
  ];
  for (const action of actions) {
    const state = reduce(START, action);
    assert.ok(state.focus !== null, `${action.do} says nothing about focus`);
  }
});

test("the tour lives outside the panel column, because it points at the page", () => {
  assert.equal(landmarkFor({ kind: "tour", step: 0 }), "[data-editor]");
  assert.equal(landmarkFor({ kind: "help", topic: "frontmatter" }), "[data-panel]");
  assert.equal(landmarkFor({ kind: "tasks" }), "[data-panel]");
});

test("every panel is announced, and a task's announcement says nothing is written", () => {
  const kinds = ["help", "tour", "templates", "tasks", "task", "vocabulary", "none"];
  const states: ShellState[] = [
    after(START, { do: "open-help" }),
    after(START, { do: "open-tour" }),
    after(START, { do: "open-templates" }),
    after(START, { do: "open-tasks" }),
    after(START, { do: "open-tasks" }, { do: "choose-task", id: "add-a-faq-entry" }),
    after(START, { do: "open-vocabulary" }),
    START,
  ];
  for (const [at, state] of states.entries()) {
    assert.equal(state.panel.kind, kinds[at]);
    assert.notEqual(announcementFor(state), "", `${kinds[at]} is silent`);
  }
  assert.match(announcementFor(states[4]!), /Nothing changes until you submit it/);
  assert.match(announcementFor(states[1]!), new RegExp(`1 of ${TOUR.length}`));
});

test("every control the guide views emit maps to an action", () => {
  // The failure this stops: a view renaming its attribute and the shell silently
  // doing nothing when the button is pressed.
  for (const attribute of [
    "data-help",
    "data-tour-next",
    "data-tour-back",
    "data-tour-skip",
    "data-tour-done",
    "data-choose-template",
    "data-choose-task",
    "data-cancel-task",
    "data-open-vocabulary",
    "data-advanced",
  ]) {
    assert.notEqual(actionFor({ [attribute]: "x" }), null, `${attribute} maps to nothing`);
  }
  assert.equal(actionFor({ "data-nothing": "x" }), null);
});

test("an empty data-help means the current context rather than a topic called empty", () => {
  assert.deepEqual(actionFor({ "data-help": "" }), { do: "open-help", topic: undefined });
  assert.deepEqual(actionFor({ "data-help": "templating" }), { do: "open-help", topic: "templating" });
});

// --- ED-74's third clause: the help is contextual -----------------------------
//
// `reduce` has handled a `context` action since the panel machine was written,
// and for one session nothing dispatched one — so `renderHelp` was always called
// with the shell's starting topic and the help was help rather than contextual
// help. A tested code path with no caller is a false green.
//
// The dispatcher itself is three lines in `editor.ts` and reads the DOM, so it is
// not testable here. What is testable is the contract it depends on: that the
// panes carry a `data-help-context`, and that what they carry is a topic `HELP`
// actually has. Either half rotting turns the help silently back into a constant.

import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { HELP } from "../src/help.ts";
import { buildModel } from "../src/model.ts";
import { formFields, validateFrontmatter } from "../src/frontmatter.ts";
import { renderFrontmatterForm } from "../src/view/form.ts";
import { renderSuggestions } from "../src/view/suggestions.ts";
import { renderSurface, unexpanded } from "../src/view/blocks.ts";

const HERE = dirname(fileURLToPath(import.meta.url));
const SCHEMA = JSON.parse(readFileSync(resolve(HERE, "../../../schemas/frontmatter.json"), "utf8"));
const CORPUS = JSON.parse(readFileSync(resolve(HERE, "fixtures/segments.json"), "utf8"));

function contextsIn(markup: string): string[] {
  return [...markup.matchAll(/data-help-context="([^"]*)"/g)].map((hit) => hit[1] ?? "");
}

test("the panes that have a help topic say which one", () => {
  const page = CORPUS.find((each: { path: string }) => each.path === "limits.md") ?? CORPUS[0];
  const surface = String(renderSurface(buildModel(page.document, page.source), unexpanded()));
  const values = { title: "Limits" };
  const form = String(
    renderFrontmatterForm({
      fields: formFields(SCHEMA),
      values,
      validation: validateFrontmatter(SCHEMA, values),
      advanced: false,
    }),
  );
  const review = String(
    renderSuggestions({
      id: "r",
      operation: "tighten",
      suggestions: [
        { id: "s", target: "0", before: "a\n", after: "b\n", rationale: "why", status: "pending", diagnostics: [] },
      ],
      withheld: [],
    }),
  );

  assert.deepEqual(contextsIn(form), ["frontmatter"]);
  assert.deepEqual(contextsIn(review), ["review"]);
  const onSurface = new Set(contextsIn(surface));
  assert.ok(onSurface.has("templating"), "a chip or a logic block says templating");
  assert.ok(onSurface.has("components"), "a component says components");
});

test("every topic a pane claims is one the help actually has", () => {
  // The rot this stops: a topic renamed in `help.ts` and not in the pane, which
  // leaves `renderHelp` drawing its "no help is written for this" state on a
  // subject that has help.
  const page = CORPUS.find((each: { path: string }) => each.path === "limits.md") ?? CORPUS[0];
  const everywhere = [
    String(renderSurface(buildModel(page.document, page.source), unexpanded())),
    String(
      renderFrontmatterForm({
        fields: formFields(SCHEMA),
        values: {},
        validation: validateFrontmatter(SCHEMA, {}),
        advanced: true,
      }),
    ),
  ];
  for (const markup of everywhere) {
    for (const topic of contextsIn(markup)) {
      assert.ok(HELP[topic] !== undefined, `no help is written for "${topic}"`);
    }
  }
});

test("a context change re-points open help and leaves the other panels alone", () => {
  // The dispatcher sends this on every focus move, so it fires far more often
  // than a click does — and it must not reopen or replace a panel the author
  // chose.
  const help = after(START, { do: "open-help" }, { do: "context", topic: "components" });
  assert.deepEqual(help.panel, { kind: "help", topic: "components" });

  const tasks = after(START, { do: "open-tasks" }, { do: "context", topic: "components" });
  assert.equal(tasks.panel.kind, "tasks", "focusing a component does not close the task list");
  assert.equal(tasks.context, "components", "but it is remembered for the next Help");
  assert.match(String(renderPanel(after(tasks, { do: "close" }, { do: "open-help" }))), /data-help-topic="components"/);
});

test("a context change never moves focus, because a focus move is what sends it", () => {
  // The bug this pins: `context` spread the previous state, so it inherited
  // whichever `focus` selector the last opened panel had set. The dispatcher
  // fires on every focus move, so tabbing into a chip re-applied that stale
  // selector and threw the author out of the surface. Everything after the
  // first help-bearing block was unreachable by keyboard.
  const opened = after(START, { do: "open-tasks" });
  assert.equal(opened.focus, "[data-choose-task]", "opening a panel does move focus");

  const moved = after(opened, { do: "context", topic: "components" });
  assert.equal(moved.focus, null, "and a focus move must not");
  assert.equal(moved.panel.kind, "tasks");

  const withHelp = after(START, { do: "open-help" }, { do: "context", topic: "components" });
  assert.equal(withHelp.focus, null, "even when it re-points the open help");
  assert.deepEqual(withHelp.panel, { kind: "help", topic: "components" });
});

test("every action that is not a focus move still says where focus goes", () => {
  // The other half: releasing focus for `context` must not release it for the
  // controls, or a panel opens without taking focus at all.
  for (const action of [
    { do: "open-help" } as const,
    { do: "open-tour" } as const,
    { do: "open-templates" } as const,
    { do: "open-tasks" } as const,
    { do: "open-vocabulary" } as const,
    { do: "close" } as const,
  ]) {
    assert.ok(reduce(START, action).focus !== null, `${action.do} lost its focus target`);
  }
});

// --- the four panels that need the open draft ---------------------------------
//
// These are the panels whose renderers existed for days with nothing opening
// them. The machine's job here is narrow — which panel, for which block, and
// what it says when the draft is absent — and the absence case is the one worth
// the most, because every editor route is `unbuilt` and absent is the state the
// editor is actually in.

import type { OpenDraft } from "../src/view/shell.ts";
import { blockIn } from "../src/view/shell.ts";

function draft(path = "limits.md"): OpenDraft {
  const page = CORPUS.find((each: { path: string }) => each.path === path) ?? CORPUS[0];
  return { model: buildModel(page.document, page.source), source: page.source, schema: SCHEMA };
}

function firstOfKind(open: OpenDraft, kind: string): string {
  const node = open.model.nodes.find((each) => each.kind === kind);
  assert.ok(node, `the fixture has a ${kind} node`);
  return node.id;
}

test("a panel that needs a draft says the draft is missing, not that it is empty", () => {
  // An empty properties form reads as "this component has no properties", which
  // is a claim about the component. The honest claim is about the build.
  for (const action of [
    { do: "open-properties", block: "0" } as const,
    { do: "edit-source", block: "0" } as const,
    { do: "open-frontmatter" } as const,
  ]) {
    const markup = String(renderPanel(reduce(START, action), null));
    assert.match(markup, /data-unserved=/, `${action.do} claims nothing about the content`);
    assert.ok(!markup.includes("<form"), `${action.do} draws no form`);
    assert.ok(!markup.includes("data-prop="), `${action.do} draws no fields`);
  }
});

test("ED-01: a component's handle opens its properties, from the draft's own props", () => {
  const open = draft();
  const id = firstOfKind(open, "component");
  const node = open.model.nodes.find((each) => each.id === id)!;
  const markup = String(renderPanel(reduce(START, { do: "open-properties", block: id }), open));
  assert.ok(markup.includes(`data-properties="${id}"`), `the pane is bound to block ${id}`);
  assert.ok(markup.includes(node.name ?? ""), "the pane names the component it is for");
  assert.ok(!markup.includes("data-unserved"), "and claims nothing is missing");
});

test("a second block's handle swaps the pane rather than closing it", () => {
  // Pressing the same handle twice closes, because that control is a toggle.
  // Pressing a different one is the author moving along the page, and closing
  // there would make the pane feel like it was fighting them.
  const first = reduce(START, { do: "open-properties", block: "0" });
  assert.deepEqual(first.panel, { kind: "properties", block: "0" });
  assert.deepEqual(reduce(first, { do: "open-properties", block: "1" }).panel, {
    kind: "properties",
    block: "1",
  });
  assert.equal(reduce(first, { do: "open-properties", block: "0" }).panel.kind, "none");
});

test("ED-03(c): the source popover goes over its block, not into the side column", () => {
  const open = draft();
  const id = firstOfKind(open, "component");
  const state = reduce(START, { do: "edit-source", block: id });
  assert.equal(landmarkFor(state.panel), `[data-block="${id}"]`);
  const markup = String(renderPanel(state, open));
  assert.match(markup, /data-source-popover=/);
  assert.match(markup, /<textarea[^>]*data-source-editor/);
  assert.ok(!markup.includes("data-prop="), "an opaque block gets bytes, not a form");
});

test("the popover carries the block's own bytes and its reason", () => {
  const open = draft();
  const id = firstOfKind(open, "component");
  const node = open.model.nodes.find((each) => each.id === id)!;
  const markup = String(renderPanel(reduce(START, { do: "edit-source", block: id }), open));
  const shown = markup.replace(/&lt;/g, "<").replace(/&gt;/g, ">").replace(/&quot;/g, '"').replace(/&amp;/g, "&");
  assert.ok(shown.includes(node.text.trimEnd()), "the bytes are the block's, unchanged");
  assert.match(markup, /class="why"/, "and it says why it is source rather than a form");
});

test("ED-11: page settings draws the schema's fields with the draft's values", () => {
  const open = draft();
  const markup = String(renderPanel(reduce(START, { do: "open-frontmatter" }), open));
  assert.match(markup, /data-frontmatter/);
  assert.match(markup, /data-field="title"/);
  assert.ok(!markup.includes("data-unserved"));
});

test("page settings with no schema says so rather than drawing a form with no rules", () => {
  // The schema is fetched, so "not yet" and "not at all" are both reachable, and
  // a form generated from nothing would accept anything while looking strict.
  const open = { ...draft(), schema: null };
  const markup = String(renderPanel(reduce(START, { do: "open-frontmatter" }), open));
  assert.match(markup, /data-unserved="ED-11"/);
  assert.ok(!markup.includes("<form"));
});

test("blockIn finds a node by id and returns nothing for one the draft lacks", () => {
  const open = draft();
  const id = firstOfKind(open, "component");
  assert.equal(blockIn(open, id)?.id, id);
  assert.equal(blockIn(open, "no-such-block"), undefined);
  assert.equal(blockIn(null, id), undefined);
});

test("every new control maps to an action, and every new panel is announced", () => {
  assert.deepEqual(actionFor({ "data-open-properties": "2/1" }), { do: "open-properties", block: "2/1" });
  assert.deepEqual(actionFor({ "data-edit-source": "0" }), { do: "edit-source", block: "0" });
  assert.deepEqual(actionFor({ "data-open-frontmatter": "" }), { do: "open-frontmatter" });
  for (const action of [
    { do: "open-properties", block: "0" } as const,
    { do: "edit-source", block: "0" } as const,
    { do: "open-frontmatter" } as const,
  ]) {
    const state = reduce(START, action);
    assert.notEqual(announcementFor(state), "", `${action.do} is silent`);
    assert.ok(state.focus !== null, `${action.do} says nothing about focus`);
  }
});

test("a change within a panel kind is still a change the author should hear", () => {
  // The bug this pins lived in `editor.ts`'s `apply`, which compared
  // `shell.panel.kind` to decide whether to announce. Advancing the tour keeps
  // the kind `"tour"`, so the step changed on screen and nothing was said; the
  // same for choosing a template and for moving the properties pane to another
  // block. The machine's job here is that the *announcement text* differs, so a
  // caller comparing the whole panel has something to say.
  const first = after(START, { do: "open-tour" });
  const second = after(first, { do: "tour-next" });
  assert.equal(first.panel.kind, second.panel.kind, "the kind is unchanged");
  assert.notEqual(announcementFor(first), announcementFor(second), "and the announcement is not");

  const blank = after(START, { do: "open-templates" });
  const chosen = after(blank, { do: "choose-template", id: "how-to" });
  assert.equal(blank.panel.kind, chosen.panel.kind);
  assert.notEqual(announcementFor(blank), announcementFor(chosen));

  // And for the properties pane the words themselves differ, once the draft is
  // there to name the component: two "Properties opened." in a row would say the
  // same thing about two different blocks.
  const open = draft();
  const ids = open.model.nodes.filter((node) => node.kind === "component").map((node) => node.id);
  assert.ok(ids.length >= 2, `the fixture has ${ids.length} components`);
  const one = after(START, { do: "open-properties", block: ids[0]! });
  const other = after(one, { do: "open-properties", block: ids[1]! });
  assert.equal(one.panel.kind, other.panel.kind);
  assert.notDeepEqual(one.panel, other.panel, "the block changed even though the kind did not");
  assert.match(announcementFor(one, open), /^Properties for \w/);
  assert.equal(
    announcementFor(one, null),
    "Properties opened.",
    "and it falls back when there is no draft to name",
  );
});
