// ED-05's toolbar, ED-71's tasks, ED-72's disclosure and ED-74's tour, help and
// templates.

import test from "node:test";
import assert from "node:assert/strict";

import { HELP, TEMPLATES, TOUR, VOCABULARY } from "../src/help.ts";
import { TASKS } from "../src/tasks.ts";
import { capIterations, previewContext, PREVIEW_DEFAULTS } from "../src/preview.ts";
import type { PreviewToolbar } from "../src/preview.ts";
import { updateANumber } from "../src/tasks.ts";
import type { ScannedPage } from "../src/bulk.ts";
import {
  askLabel,
  describeContext,
  renderCappedRows,
  renderContextToolbar,
  renderHelp,
  renderProposal,
  renderTaskForm,
  renderTaskList,
  renderTemplatePicker,
  renderTourStep,
  renderVocabulary,
  termsIn,
} from "../src/view/guides.ts";

const CHOICES = { versions: ["2.0", "1.9"], locales: ["en", "de"], readerGroups: ["staff", "beta"] };
const NOTHING: PreviewToolbar = { readerGroups: [] };

// --- ED-05 ------------------------------------------------------------------

test("every axis the project defines gets a control, and Any is the default", () => {
  const markup = String(renderContextToolbar(NOTHING, CHOICES));
  assert.match(markup, /data-context-axis="version"/);
  assert.match(markup, /data-context-axis="locale"/);
  assert.match(markup, /data-context-group="staff"/);
  assert.match(markup, /<option value="" selected>Any<\/option>/);
  assert.ok(!markup.includes('value="2.0" selected'), "no version is chosen for the author");
});

test("the chosen value is the selected one, and no other", () => {
  const markup = String(
    renderContextToolbar({ version: "1.9", locale: "de", readerGroups: ["beta"] }, CHOICES),
  );
  assert.match(markup, /<option value="1\.9" selected>/);
  assert.ok(!markup.includes('value="2.0" selected'));
  assert.match(markup, /data-context-group="beta" checked/);
  assert.ok(!markup.includes('data-context-group="staff" checked'));
  assert.ok(!markup.includes('value="" selected'), "Any is not selected once a version is");
});

test("a reset select posts an empty string, and Any is selected for it", () => {
  // Reachable from the pane rather than from the model: reading a select whose
  // value is Any gives `""`, not `undefined`, so a toolbar rebuilt from the DOM
  // arrives here with `version: ""`. Without the empty-string case Any loses its
  // mark and the picker looks as though nothing is selected at all.
  const markup = String(renderContextToolbar({ version: "", readerGroups: [] }, CHOICES));
  assert.match(markup, /<option value="" selected>Any<\/option>/);
  assert.equal((markup.match(/ selected>/g) ?? []).length, 2, "one Any per axis, and nothing else");
  assert.equal(describeContext({ version: "", readerGroups: [] }), "Showing this page as any reader sees it.");
});

test("an axis the project does not define is not drawn", () => {
  // A locale picker on a single-language project is a control that cannot
  // change anything, and every one of those teaches an author to ignore the
  // toolbar.
  const markup = String(renderContextToolbar(NOTHING, { versions: ["1.0"], locales: [], readerGroups: [] }));
  assert.match(markup, /data-context-axis="version"/);
  assert.ok(!markup.includes('data-context-axis="locale"'));
  assert.ok(!markup.includes("<fieldset"), "no reader groups, no fieldset");
});

test("the toolbar says which context it is showing, in a live region", () => {
  // Changing the context redraws the preview without moving focus: a sighted
  // author sees it, and without the live region a screen reader user gets
  // nothing at all.
  const markup = String(renderContextToolbar({ version: "1.9", readerGroups: ["beta"] }, CHOICES));
  assert.match(markup, /data-context-note role="status" aria-live="polite"/);
  assert.match(markup, /Showing this page for version 1\.9, a reader in beta\./);
});

test("the sentence describes exactly the axes the context carries", () => {
  // The claim the note makes has to be the claim `previewContext` sends, or the
  // preview is of one context and the caption is of another.
  const toolbar: PreviewToolbar = { version: "2.0", readerGroups: ["staff"] };
  const context = previewContext(toolbar, {});
  const said = describeContext(toolbar);
  assert.ok(said.includes("version 2.0"), said);
  assert.ok(!said.includes("language"), "no locale is set, so none is claimed");
  assert.ok(!("locale" in context), "and none is sent");
  assert.equal(describeContext(NOTHING), "Showing this page as any reader sees it.");
});

test("a capped preview names the whole expansion and the cap", () => {
  const capped = capIterations(Array.from({ length: 900 }, (_, at) => at), PREVIEW_DEFAULTS);
  const markup = String(renderCappedRows(capped));
  assert.match(markup, /Showing 50 of 900 rows/);
  assert.match(markup, /at most 50/);
  assert.match(markup, /data-show-all-rows>Show all 900</);
  assert.match(markup, /data-truncated/);
});

test("an uncapped preview offers no show-all and claims no truncation", () => {
  const markup = String(renderCappedRows(capIterations([1, 2, 3], PREVIEW_DEFAULTS)));
  assert.match(markup, /3 rows\./);
  assert.ok(!markup.includes("data-truncated"));
  assert.ok(!markup.includes("data-show-all-rows"));
  assert.equal(String(renderCappedRows(capIterations([1], PREVIEW_DEFAULTS))).includes("1 row."), true);
});

// --- ED-72 ------------------------------------------------------------------

test("the plain word is the heading and the git term is inside the disclosure", () => {
  // ED-72's shape, and it only works one way round: an author who does not know
  // what a branch is must be able to read the editor without meeting one.
  const markup = String(renderVocabulary(false));
  for (const term of Object.keys(VOCABULARY)) {
    assert.match(markup, new RegExp(`data-term="${term}"`), `${term} is listed`);
    assert.match(markup, new RegExp(`<dt>${VOCABULARY[term]!.plain}</dt>`), `${term} leads with the plain word`);
  }
  assert.match(markup, /In git: branch/);
  assert.match(markup, /<summary>What these words mean<\/summary>/);
  assert.ok(!/<summary>[^<]*branch/.test(markup), "the summary does not lead with git");
});

test("the disclosure is closed by default and open when advanced is on", () => {
  assert.ok(!String(renderVocabulary(false)).includes("<details class=\"vocabulary\" data-vocabulary open"));
  assert.match(String(renderVocabulary(true)), /data-vocabulary open/);
});

// --- ED-74 ------------------------------------------------------------------

test("a tour step points at something and does not trap the author in front of it", () => {
  const markup = String(renderTourStep(0));
  assert.match(markup, /aria-modal="false"/, "the step must not hide what it describes");
  assert.match(markup, new RegExp(`data-tour-target="${TOUR[0]!.target.replace(/[[\]]/g, "\\$&")}"`));
  assert.match(markup, /Step 1 of 6/);
  assert.ok(!markup.includes("data-tour-back"), "nothing is before the first step");
  assert.match(markup, /data-tour-skip/);
});

test("skip is on every step, and the last step finishes rather than continuing", () => {
  const last = String(renderTourStep(TOUR.length - 1));
  assert.match(last, /data-tour-skip/);
  assert.match(last, /data-tour-done/);
  assert.ok(!last.includes("data-tour-next"), "there is no next step to offer");
  assert.match(last, /data-tour-back/);
  assert.equal(String(renderTourStep(TOUR.length)), "", "a step past the end renders nothing");
});

test("a topic with no help written says so instead of opening empty", () => {
  const markup = String(renderHelp("no-such-topic", false));
  assert.match(markup, /data-help-missing/);
  assert.match(markup, /No help is written for this yet/);
  assert.ok(!markup.includes("<h2"), "and it does not invent a title");
});

test("help shows the vocabulary its own prose uses, and only under advanced", () => {
  // The alternative is a disclosure that explains "publish" under the front
  // matter help, and noise is what makes a disclosure go unread.
  const plain = String(renderHelp("review", false));
  assert.ok(!plain.includes("data-help-terms"), "no git terms until asked for");
  const advanced = String(renderHelp("review", true));
  assert.match(advanced, /data-help-terms/);
  assert.match(advanced, /data-term="review"/);
  assert.ok(!advanced.includes('data-term="undo"'), "the review help does not mention undo");
});

test("termsIn finds the words a body uses and not the ones it does not", () => {
  assert.deepEqual(termsIn("Suggesting sends a draft for review."), ["draft", "suggest", "review"]);
  assert.deepEqual(termsIn("Nothing relevant here."), []);
  assert.deepEqual(termsIn(HELP["frontmatter"]!.body), []);
});

test("a term has to start a word: a preview is not a review", () => {
  // The prefix boundary, and it is only the prefix: an inflection counts, so
  // "Suggesting" is `suggest` and "publishes" is `publish`, while "preview" and
  // "undoing"-shaped words that merely contain a term are not it. Without the
  // boundary the templating help would claim to explain review.
  assert.deepEqual(termsIn("Check the preview before you send it."), []);
  assert.deepEqual(termsIn("Redrafting is not the same as a draft."), ["draft"]);
  assert.deepEqual(termsIn("Publishes it for every reader."), ["publish"]);
});

test("every template says which of the four documentation kinds it is", () => {
  // Choosing between a tutorial and a how-to guide is the decision an
  // occasional author most often gets wrong, so the picker has to make it
  // decidable rather than just naming both.
  const markup = String(renderTemplatePicker());
  for (const template of TEMPLATES) {
    assert.match(markup, new RegExp(`data-template="${template.id}"`), `${template.id} is offered`);
    assert.match(markup, new RegExp(`data-kind="${template.kind}"`));
    assert.ok(markup.includes(template.description), `${template.id} explains itself`);
  }
  assert.match(markup, /Teaches a beginner by doing/);
});

test("the chosen template is the current one", () => {
  const markup = String(renderTemplatePicker("reference"));
  assert.match(markup, /data-template="reference" data-kind="reference" aria-current="true"/);
  assert.equal((markup.match(/aria-current/g) ?? []).length, 1);
});

// --- ED-71 ------------------------------------------------------------------

test("each task says what it will show before anything changes", () => {
  const markup = String(renderTaskList());
  for (const task of TASKS) {
    assert.match(markup, new RegExp(`data-choose-task="${task.id}"`), `${task.id} is offered`);
    assert.ok(markup.includes(`Shows ${task.shows} before anything changes.`), `${task.id} says what it shows`);
  }
});

test("a task's form asks for exactly what the task names", () => {
  const markup = String(renderTaskForm("update-a-number"));
  assert.match(markup, /data-task-field="fact"/);
  assert.match(markup, /data-task-field="value"/);
  assert.equal((markup.match(/data-task-field=/g) ?? []).length, 2, "and nothing else");
  assert.match(markup, /Show me the pages that show this number/);
});

test("every field of every task has a label tied to its control", () => {
  for (const task of TASKS) {
    const markup = String(renderTaskForm(task.id));
    for (const ask of task.asks) {
      const id = `task-${task.id}-${ask}`;
      assert.match(markup, new RegExp(`<label for="${id}">`), `${task.id}/${ask} has a label`);
      assert.match(markup, new RegExp(`id="${id}"`), `${task.id}/${ask} has that id`);
      assert.ok(!String(askLabel(ask)).includes("undefined"));
    }
  }
});

test("an unknown task is refused rather than drawn empty", () => {
  const markup = String(renderTaskForm("rename-the-world"));
  assert.match(markup, /data-unknown-task="rename-the-world"/);
  assert.ok(!markup.includes("<input"), "no form for a task that does not exist");
});

test("ED-71: the proposal shows the affected pages, not only the file it writes", () => {
  // The row's own example: a fact used on four pages. The write is one file, and
  // a proposal that showed only its writes would hide exactly the thing the
  // author is being asked to judge.
  const pages = ["a.md", "b.md", "c.md", "d.md"].map((path) => page(path, 'Costs {{ fact("price") }} today.\n'));
  const proposal = updateANumber(pages, {
    fact: "price",
    value: "12",
    factFile: "facts/pricing.json",
    factSource: '{ "price": 9 }\n',
  });
  const markup = String(renderProposal(proposal));
  assert.equal(proposal.pages.length, 4, "the fixture is the row's four pages");
  assert.match(markup, /4 pages affected\./);
  for (const path of pages.map((each) => each.path)) {
    assert.ok(markup.includes(`<code>${path}</code>`), `${path} is listed`);
  }
  assert.match(markup, /<code>facts\/pricing\.json<\/code>/);
  assert.match(markup, /data-writes/);
});

test("the proposal commits to nothing, and says so", () => {
  const proposal = updateANumber([], {
    fact: "price",
    value: "12",
    factFile: "facts/pricing.json",
    factSource: '{ "price": 9 }\n',
  });
  const markup = String(renderProposal(proposal));
  assert.match(markup, /Suggest this/);
  assert.ok(!markup.includes("Publish"), "a task never publishes");
  assert.match(markup, /Finishing a task publishes nothing\./);
  assert.match(markup, /No page shows this\./);
  assert.ok(!markup.includes("data-affected>"), "no empty list of affected pages");
});

/** A page for the task fixtures; the document is unused by these tasks. */
function page(path: string, source: string): ScannedPage {
  return { path, source, document: { segments: [], source: 0 } };
}
