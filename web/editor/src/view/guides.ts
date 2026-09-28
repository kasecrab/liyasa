// ED-05's preview-context toolbar, ED-72's vocabulary disclosure, ED-74's tour,
// contextual help and page templates, and ED-71's guided tasks.
//
// These are the surfaces for somebody who uses the editor once a quarter, and
// what they have in common is that they must not overstate what they know. A
// guided task ends in a proposal rather than a write, the help panel says when
// no help is written, and the toolbar names the context it is showing instead of
// implying there is only one.

import { html, raw } from "../escape.ts";
import type { Fragment } from "../escape.ts";
import { HELP, TEMPLATES, TOUR, VOCABULARY, say } from "../help.ts";
import type { PageTemplate } from "../help.ts";
import { TASKS, findTask } from "../tasks.ts";
import type { Proposal, TaskSpec } from "../tasks.ts";
import { PREVIEW_DEFAULTS } from "../preview.ts";
import type { CappedRows, PreviewLimits, PreviewToolbar } from "../preview.ts";

// --- ED-05: the preview context ---------------------------------------------

/** What the open project offers on each axis, from its own configuration. */
export interface ContextChoices {
  versions: string[];
  locales: string[];
  readerGroups: string[];
}

/**
 * The toolbar ED-05 asks for: version, locale and sample reader groups.
 *
 * Every axis has an explicit "Any", and it is the default. A toolbar that
 * silently selected the first version would show one answer while implying it
 * was the only one — and `previewContext` leaves an unselected axis out of the
 * context entirely, because an empty string is a version named `""` and
 * `by_version[""]` is a lookup the build never makes.
 *
 * An axis the project does not define is not drawn at all. A locale picker on a
 * single-language project is a control that cannot change anything.
 */
export function renderContextToolbar(toolbar: PreviewToolbar, choices: ContextChoices): Fragment {
  return html`<div class="context-toolbar" data-context-toolbar role="group" aria-label="Preview context">
    ${axis("version", "Version", choices.versions, toolbar.version)}
    ${axis("locale", "Language", choices.locales, toolbar.locale)}
    ${choices.readerGroups.length === 0
      ? null
      : html`<fieldset class="context-groups">
          <legend>Preview as a reader in</legend>
          ${choices.readerGroups.map((group) => readerGroup(group, toolbar.readerGroups.includes(group)))}
        </fieldset>`}
    <p class="context-note" data-context-note role="status" aria-live="polite">${describeContext(toolbar)}</p>
  </div>`;
}

function axis(name: string, label: string, values: string[], chosen: string | undefined): Fragment {
  if (values.length === 0) return html``;
  const id = `context-${name}`;
  return html`<label class="context-field" for="${id}">${label}
    <select id="${id}" data-context-axis="${name}">
      <option value=""${chosen === undefined || chosen === "" ? raw(" selected") : null}>Any</option>
      ${values.map(
        (value) => html`<option value="${value}"${value === chosen ? raw(" selected") : null}>${value}</option>`,
      )}
    </select>
  </label>`;
}

function readerGroup(group: string, on: boolean): Fragment {
  // Attributes on one line: a wrapped template puts a newline between two of
  // them, and then every assertion about the pair has to know where the
  // renderer happened to break the line.
  return html`<label class="context-group"><input type="checkbox" data-context-group="${group}"${on ? raw(" checked") : null} /> ${group}</label>`;
}

/**
 * What the preview is currently showing, said out loud.
 *
 * In the live region, because changing the context changes what the page says
 * without moving focus, and a sighted author sees the preview redraw while a
 * screen reader user gets nothing.
 */
export function describeContext(toolbar: PreviewToolbar): string {
  const parts: string[] = [];
  if (toolbar.version) parts.push(`version ${toolbar.version}`);
  if (toolbar.locale) parts.push(`language ${toolbar.locale}`);
  if (toolbar.readerGroups.length > 0) parts.push(`a reader in ${toolbar.readerGroups.join(" and ")}`);
  return parts.length === 0
    ? "Showing this page as any reader sees it."
    : `Showing this page for ${parts.join(", ")}.`;
}

/**
 * ED-05's capped loop preview and its "show all" control.
 *
 * The total is the whole expansion, not the drawn part: "50 of 900" is the
 * sentence that tells an author the preview is partial, and "50 rows" is not.
 * The cap is also named, so the number does not look like a property of their
 * data.
 */
export function renderCappedRows<T>(capped: CappedRows<T>, limits: PreviewLimits = PREVIEW_DEFAULTS): Fragment {
  const rows = (count: number): string => `${count} row${count === 1 ? "" : "s"}`;
  if (!capped.truncated) {
    return html`<p class="expanded-count" data-expanded-count>${rows(capped.total)}.</p>`;
  }
  return html`<p class="expanded-count" data-expanded-count data-truncated>Showing ${capped.shown.length} of ${rows(capped.total)}. While you type the editor draws at most ${limits.maxIterations}.
    <button type="button" data-show-all-rows>Show all ${capped.total}</button>
  </p>`;
}

// --- ED-72: the vocabulary disclosure ---------------------------------------

/**
 * ED-72's "advanced" disclosure.
 *
 * > git details available under an "advanced" disclosure for those who want them
 *
 * The plain word is always the heading and the git term is always inside the
 * disclosure — never the other way round, and never both in the heading. An
 * author who does not know what a branch is should be able to read the whole
 * editor without meeting one, and an author who does know should be able to find
 * out what the editor is actually doing in one place rather than inferring it.
 */
export function renderVocabulary(advanced: boolean): Fragment {
  const terms = Object.keys(VOCABULARY);
  return html`<details class="vocabulary" data-vocabulary${advanced ? raw(" open") : null}>
    <summary>What these words mean</summary>
    <dl>
      ${terms.map(
        (term) => html`<div class="term" data-term="${term}">
          <dt>${VOCABULARY[term]!.plain}</dt>
          <dd>${VOCABULARY[term]!.explains}</dd>
          <dd class="term-git">In git: ${VOCABULARY[term]!.git}</dd>
        </div>`,
      )}
    </dl>
  </details>`;
}

// --- ED-74: the tour, help and templates ------------------------------------

/**
 * One step of the onboarding tour.
 *
 * `aria-modal="false"` deliberately: the step points at something on screen and
 * the author should be able to look at what it points at. A modal tour makes the
 * thing it is describing unreachable for as long as it describes it.
 *
 * Skip is on every step rather than only the first. A tour you can only leave by
 * finishing is a tour people learn to dismiss before reading.
 */
export function renderTourStep(index: number): Fragment {
  const step = TOUR[index];
  if (!step) return html``;
  const id = `tour-${step.id}`;
  return html`<div class="tour-step" role="dialog" aria-modal="false" aria-labelledby="${id}-title" data-tour-step="${step.id}" data-tour-target="${step.target}">
    <h2 id="${id}-title">${step.title}</h2>
    <p>${step.body}</p>
    <p class="tour-progress">Step ${index + 1} of ${TOUR.length}</p>
    <div class="form-actions">
      ${index > 0 ? html`<button type="button" data-tour-back>Back</button>` : null}
      ${index + 1 < TOUR.length
        ? html`<button type="button" class="primary" data-tour-next>Next</button>`
        : html`<button type="button" class="primary" data-tour-done>Finish</button>`}
      <button type="button" data-tour-skip>Skip the tour</button>
    </div>
  </div>`;
}

/**
 * ED-74's contextual help for whatever the author is looking at.
 *
 * A topic with no help written says so. The alternative — a panel that opens
 * empty, or one that silently does not open — teaches an author that help is
 * not worth asking for, which costs more than the missing paragraph.
 *
 * The vocabulary rows are the terms this topic's own prose uses, not all of
 * them: a disclosure that explains "publish" under the front matter help is
 * noise, and noise is what makes a disclosure go unread.
 */
export function renderHelp(topic: string, advanced: boolean): Fragment {
  const entry = HELP[topic];
  if (!entry) {
    return html`<aside class="help" data-help-topic="${topic}" data-help-missing aria-label="Help">
      <p class="empty">No help is written for this yet. The reference has more: <a href="/docs/">the documentation</a>.</p>
    </aside>`;
  }
  // Title and body both: the heading is prose the author reads, and the review
  // help says "review" in its heading and "suggestion" in its text.
  const mentioned = termsIn(`${entry.title} ${entry.body}`);
  return html`<aside class="help" data-help-topic="${entry.id}" aria-labelledby="help-${entry.id}-title">
    <h2 id="help-${entry.id}-title">${entry.title}</h2>
    <p>${entry.body}</p>
    ${!advanced || mentioned.length === 0
      ? null
      : html`<dl class="help-terms" data-help-terms>
          ${mentioned.map(
            (term) => html`<div class="term" data-term="${term}">
              <dt>${say(term, true)}</dt>
              <dd>${VOCABULARY[term]!.explains}</dd>
            </div>`,
          )}
        </dl>`}
  </aside>`;
}

/** The vocabulary words a piece of help prose actually uses. */
export function termsIn(prose: string): string[] {
  const lower = prose.toLowerCase();
  return Object.keys(VOCABULARY).filter((term) =>
    new RegExp(`\\b${VOCABULARY[term]!.plain.toLowerCase()}`).test(lower),
  );
}

/**
 * ED-74's template picker.
 *
 * The kind is shown beside the name because choosing between "Tutorial" and
 * "How-to guide" is the decision an occasional author most often gets wrong,
 * and the description is what makes it decidable. The body each template
 * carries is guidance the author replaces, not lorem ipsum and not a comment
 * they have to find and delete.
 */
export function renderTemplatePicker(selected?: string): Fragment {
  return html`<section class="templates" data-templates aria-label="Page templates">
    <h2>Start from a template</h2>
    <ul class="template-list">
      ${TEMPLATES.map((template) => renderTemplateChoice(template, template.id === selected))}
    </ul>
  </section>`;
}

function renderTemplateChoice(template: PageTemplate, selected: boolean): Fragment {
  return html`<li class="template" data-template="${template.id}" data-kind="${template.kind}"${selected ? raw(' aria-current="true"') : null}>
    <button type="button" data-choose-template="${template.id}"><strong>${template.label}</strong> <span class="template-kind">${TEMPLATE_KIND[template.kind]}</span></button>
    <p class="field-help">${template.description}</p>
  </li>`;
}

const TEMPLATE_KIND: Record<PageTemplate["kind"], string> = {
  tutorial: "Teaches a beginner by doing",
  "how-to": "Solves one problem for somebody who knows the product",
  reference: "Describes what something is, exhaustively",
  explanation: "Explains why it works this way",
  other: "Something else",
};

// --- ED-71: the guided tasks ------------------------------------------------

/** The five tasks, each saying what it will show before it does anything. */
export function renderTaskList(selected?: string): Fragment {
  return html`<section class="tasks" data-tasks aria-label="Guided tasks">
    <h2>What would you like to do?</h2>
    <ul class="task-list">
      ${TASKS.map((task) => renderTaskChoice(task, task.id === selected))}
    </ul>
  </section>`;
}

function renderTaskChoice(task: TaskSpec, selected: boolean): Fragment {
  return html`<li class="task" data-task="${task.id}"${selected ? raw(' aria-current="true"') : null}>
    <button type="button" data-choose-task="${task.id}">${task.label}</button>
    <p class="field-help">Shows ${task.shows} before anything changes.</p>
  </li>`;
}

/** The label for one thing a task asks for. */
export function askLabel(ask: string): string {
  const named: Record<string, string> = {
    fact: "Which number",
    value: "Its new value",
    from: "The old name",
    to: "The new name",
    asset: "Which image",
    file: "The new file",
    question: "The question",
    answer: "The answer",
    date: "When it happened",
    summary: "What changed",
  };
  return named[ask] ?? ask.charAt(0).toUpperCase() + ask.slice(1);
}

/**
 * A task's first screen.
 *
 * It asks for exactly what `TaskSpec.asks` names and nothing else, and it says
 * what pressing the button will do — show, not apply. The proposal is a separate
 * screen because ED-71's guarantee is that the author sees the blast radius
 * before agreeing to it, and a form that both collects and commits has nowhere
 * to put that.
 */
export function renderTaskForm(id: string): Fragment {
  const task = findTask(id);
  if (!task) {
    return html`<p class="empty" data-unknown-task="${id}">There is no task called ${id}.</p>`;
  }
  return html`<form class="task-form" data-task-form="${task.id}" aria-label="${task.label}">
    <h2>${task.label}</h2>
    ${task.asks.map((ask) => taskField(task.id, ask))}
    <div class="form-actions">
      <button type="submit" class="primary" data-task-preview="${task.id}">Show me ${task.shows}</button>
    </div>
  </form>`;
}

function taskField(task: string, ask: string): Fragment {
  const id = `task-${task}-${ask}`;
  return html`<div class="field"><label for="${id}">${askLabel(ask)}</label> <input type="text" id="${id}" name="${ask}" data-task-field="${ask}" required /></div>`;
}

/**
 * ED-71's proposal.
 *
 * > then the proposal updates the fact source and shows the four affected pages
 *
 * So: the affected pages first, the files written second, and both always —
 * even when the proposal writes one file, because a change to a fact reaches
 * pages that the file list does not name. A proposal that showed only its writes
 * would hide exactly the thing an author is being asked to judge.
 */
export function renderProposal(proposal: Proposal): Fragment {
  const pages = proposal.pages.length;
  return html`<section class="proposal" data-proposal="${proposal.task}" aria-label="What this will change">
    <h2>${proposal.summary}</h2>
    <p class="proposal-scope" data-affected-count>${pages === 0 ? "No page shows this." : `${pages} page${pages === 1 ? "" : "s"} affected.`}</p>
    ${pages === 0
      ? null
      : html`<ul class="affected" data-affected>
          ${proposal.pages.map((page) => html`<li><code>${page}</code></li>`)}
        </ul>`}
    ${proposal.writes.length === 0
      ? null
      : html`<div class="proposal-writes" data-writes>
          <h3>What this writes</h3>
          <ul>
            ${proposal.writes.map((write) => html`<li><code>${write.path}</code></li>`)}
          </ul>
        </div>`}
    ${proposal.plan === null || proposal.plan.matches.length === 0
      ? null
      : html`<p class="field-help" data-match-count>${proposal.plan.matches.length} occurrence${proposal.plan.matches.length === 1 ? "" : "s"} in all.</p>`}
    <div class="form-actions">
      <button type="button" class="primary" data-submit-task="${proposal.task}">Suggest this</button>
      <button type="button" data-cancel-task>Cancel</button>
    </div>
    <p class="field-help">This becomes a suggestion somebody reviews. Finishing a task publishes nothing.</p>
  </section>`;
}
