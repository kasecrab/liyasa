// The editor application.
//
// This is the only module that touches the document, the network, storage or
// the WebAssembly module. Everything else is a function from state to markup
// or to `SegmentEdit`s, which is why `test/` needs no browser: a test against a
// stand-in DOM proves the stand-in works. `web/dashboard/src/dashboard.ts`
// took the same shape, for the same reason.
//
// Nothing here runs under `node --test`. What is testable about the shell is
// the markup its renderers produce, and those are pure and live beside the
// state they render.

import { html, raw } from "./escape.ts";
import { isRecord } from "./text.ts";
import type { Fragment } from "./escape.ts";
import { buildModel, editBlock, editBlocks, serializeModel } from "./model.ts";
import type { EditorModel, MarkdownBlock } from "./model.ts";
import { decorate, placeDiagnostics, tokenize } from "./source.ts";
import { SLASH_COMMANDS, matchShortcut, moveBlock, slashInsert, slashMatches } from "./commands.ts";
import { htmlToMarkdown, imagesIn, pasteSource } from "./paste.ts";
import { PREVIEW_DEFAULTS, capBytes, capIterations, chipSource, previewContext, previewLimits } from "./preview.ts";
import { formFields, parseFrontmatter, validateFrontmatter, writeFrontmatter } from "./frontmatter.ts";
import { createPage, deletePage, duplicatePage, movePage, navigationOf, renamePage, reorderNavigation, routeOf } from "./pages.ts";
import { applyPlan, applyTag, changeFactReference, findReplace, moveGroup } from "./bulk.ts";
import { darkPartner, deleteRefusal, imageDirective, searchAssets, sniff, transformQuery, validateUpload } from "./media.ts";
import { ageOf, alsoOpenElsewhere, draftBranch, merge3, pagesTouched, save, searchDrafts, seenTab } from "./drafts.ts";
import { call, endpointPath, findEndpoint, unservedBy } from "./api.ts";
import { may, primaryAction, refusal } from "./roles.ts";
import type { Grant } from "./roles.ts";
import { assignReviewers, decide, ownersFor, parseDocowners, publishPlan, queueRows, staleReminders } from "./review.ts";
import { OPERATIONS, acceptedEdits, pendingCount, rejectAll, runPayload, screen, setStatus } from "./agent.ts";
import { ACTIVITY_KINDS, KIND_LABEL, byDay, counts, feed } from "./activity.ts";
import { TASKS, addAFaqEntry, quickFixProposal, recordAChangelogEntry, renameEverywhere, replaceAScreenshot, suggestEditUrl, updateANumber } from "./tasks.ts";
import { HELP, TEMPLATES, TOUR, VOCABULARY, pageFromTemplate, say } from "./help.ts";
import { LANDMARKS, SHORTCUTS, chartTable, motionDuration, reviewAnnouncement, saveAnnouncement, validationAnnouncement } from "./a11y.ts";
import { messageFor, shownFor } from "./messages.ts";
import { EditorSession, PreviewHold, sessionNonce } from "./session.ts";
import { renderFrontmatterForm, valueFromControl } from "./view/form.ts";
import { byCategory, formFor, propFromControl, propsToDirective, renderProperties } from "./view/properties.ts";
import { highlight, opaqueReason, renderSourceMode, renderSourcePopover } from "./view/source-mode.ts";
import { renderSuggestion, renderSuggestions, suggestionAnnouncement } from "./view/suggestions.ts";
import { renderActivity, renderConflict, renderDrafts, renderEmpty, renderMedia } from "./view/panes.ts";
import { chipTooltip, expressionCompletions, renderBlock, renderChip, renderCode, renderComponent, renderExpressionEditor, renderLogicBlock, renderNode, renderOpaque, renderSurface, unexpanded } from "./view/blocks.ts";
import { askLabel, describeContext, renderCappedRows, renderContextToolbar, renderHelp, renderProposal, renderTaskForm, renderTaskList, renderTemplatePicker, renderTourStep, renderVocabulary, termsIn } from "./view/guides.ts";
import { START, actionFor, announcementFor, landmarkFor, reduce, renderPanel } from "./view/shell.ts";
import type { Action, OpenDraft, ShellState } from "./view/shell.ts";

/** What the shell holds while a draft is open. */
interface State {
  mode: "visual" | "source";
  advanced: boolean;
  grant: Grant;
  model: EditorModel | null;
  source: string;
  path: string;
  version: number;
  baseText: string;
  announcement: string;
}

const state: State = {
  mode: "visual",
  advanced: false,
  grant: { role: "contributor" },
  model: null,
  source: "",
  path: "",
  version: 0,
  baseText: "",
  announcement: "",
};

// --- rendering --------------------------------------------------------------

/**
 * Where a code's help page may be linked from.
 *
 * A `Diagnostic`'s `url` is a string in the payload — generated from the code
 * on the Rust side, but a *value* by the time it reaches here, and this editor
 * renders diagnostics that came over the network. Putting it in an `href`
 * unchecked means a crafted response can run `javascript:` in the editor's own
 * origin, on a link the author has every reason to click.
 */
const HELP_ORIGIN = "https://kasecrab.github.io/";

function helpLink(url: string): string | null {
  return url.startsWith(HELP_ORIGIN) ? url : null;
}

/**
 * The problems pane.
 *
 * Two lines per problem: ED-73's plain-language headline, and the diagnostic's
 * own message underneath when it says something the headline cannot — which
 * field, which prop, which file. The headline alone would tell an author that
 * "a setting has the wrong kind of value" without saying which setting.
 *
 * A code with no plain-language entry — a build-side one reaching this pane
 * through a preview or a verification result — shows its own message as the
 * headline. That is more specific than the registry title the editor used to
 * look up, and it needs nothing derived from `codes.toml` (RFC 2433).
 */
export function renderProblems(source: string, diagnostics: Parameters<typeof placeDiagnostics>[1]): Fragment {
  const placed = placeDiagnostics(source, diagnostics);
  if (placed.length === 0) return html`<p class="empty">No problems found.</p>`;
  return html`<ul class="problems">
    ${placed.map((entry) => {
      const shown = shownFor(entry.diagnostic);
      const link = helpLink(entry.diagnostic.url);
      return html`<li class="problem problem-${entry.diagnostic.severity}">
        <span class="where">Line ${entry.from.line}</span>
        <span class="what">
          ${shown.headline}
          ${shown.detail === null ? null : html`<span class="detail">${shown.detail}</span>`}
        </span>
        ${shown.fix ? html`<button type="button" data-fix="${shown.fix.action}">${shown.fix.label}</button>` : null}
        ${link
          ? html`<a class="code" href="${link}">${entry.diagnostic.code}</a>`
          : html`<span class="code">${entry.diagnostic.code}</span>`}
      </li>`;
    })}
  </ul>`;
}

/**
 * The toolbar.
 *
 * Its main button is one this person can press (ED-75), and the words are
 * ED-72's: `say()` adds the git term only when the advanced disclosure is on.
 */
export function renderToolbar(grant: Grant, mode: "visual" | "source", advanced: boolean): Fragment {
  const primary = primaryAction(grant);
  return html`<header class="toolbar" role="banner" aria-label="Editor toolbar">
    <button type="button" data-mode-switch aria-keyshortcuts="Control+E">
      ${mode === "visual" ? "Source" : "Visual"}
    </button>
    <button type="button" data-action="${primary.action}" class="primary">${primary.label}</button>
    <button type="button" data-help aria-keyshortcuts="?">Help</button>
    <label class="advanced">
      <input type="checkbox" data-advanced ${advanced ? raw("checked") : null} />
      Show ${say("draft", advanced)} details
    </label>
  </header>`;
}

/** The keyboard-shortcut sheet, which is also ED-80's evidence. */
export function renderShortcuts(): Fragment {
  return html`<table class="shortcuts">
    <caption>Keyboard shortcuts</caption>
    <thead><tr><th scope="col">Keys</th><th scope="col">Does</th><th scope="col">Where</th></tr></thead>
    <tbody>
      ${SHORTCUTS.map(
        (shortcut) => html`<tr><td><kbd>${shortcut.keys}</kbd></td><td>${shortcut.description}</td><td>${shortcut.scope}</td></tr>`,
      )}
    </tbody>
  </table>`;
}

// --- the shell --------------------------------------------------------------

function mount(): void {
  const root = document.querySelector("[data-editor]");
  if (!root) return;

  const reduced = globalThis.matchMedia?.("(prefers-reduced-motion: reduce)").matches ?? false;
  root.setAttribute("style", `--motion: ${motionDuration(180, reduced)}ms`);

  for (const landmark of LANDMARKS) {
    const region = root.querySelector(`[data-landmark="${landmark.label}"]`);
    region?.setAttribute("role", landmark.role);
    region?.setAttribute("aria-label", landmark.label);
  }

  restoreTourSeen();
  document.addEventListener("click", onClick);
  document.addEventListener("keydown", onKey);
  document.addEventListener("focusin", onFocus);
  apply({ do: "first-visit", seen: shell.tourSeen });
  openEmbeddedDraft();

  announce(state.model === null ? "Editor ready" : `${state.path || "A draft"} opened.`);
}

// --- the panels -------------------------------------------------------------
//
// Every decision about which panel opens, what closes it, where it goes and what
// is announced is in `view/shell.ts` and tested there. What is left here is DOM:
// read the attributes off the clicked control, apply the result, move focus.

let shell: ShellState = START;

/**
 * The draft on screen, or `null` before one arrives.
 *
 * Separate from `shell` because the panel machine is a pure function of actions
 * and this is a function of the network. Keeping them apart is what lets every
 * panel decision be tested without a document and every document be rendered
 * without replaying a click.
 */
let open: OpenDraft | null = null;

/**
 * Opens a draft: the one entry point between "the editor has a document" and
 * everything that draws one.
 *
 * It reads the draft out of the page rather than fetching it. The route that
 * will serve `/_liyasa/editor/` already has the parsed document in hand when it
 * renders the shell, so embedding it costs nothing and saves the editor a round
 * trip before it can show anything — and it means the first paint is not behind
 * a request. `api.ts` marks every editor route `unbuilt`, so today nothing
 * embeds one and this returns having done nothing.
 */
function openEmbeddedDraft(): void {
  const carrier = document.querySelector('script[type="application/json"][data-draft]');
  if (!carrier?.textContent) return;
  let payload: unknown;
  try {
    payload = JSON.parse(carrier.textContent);
  } catch {
    // A malformed payload is the server's bug and the author's problem either
    // way, so it is said out loud rather than swallowed into a blank editor.
    announce("This draft could not be read. Nothing has been opened.");
    return;
  }
  if (!isRecord(payload) || typeof payload["source"] !== "string" || !isRecord(payload["document"])) {
    announce("This draft could not be read. Nothing has been opened.");
    return;
  }
  const source = payload["source"];
  const path = typeof payload["path"] === "string" ? payload["path"] : "";
  state.source = source;
  state.path = path;
  state.baseText = source;
  state.model = buildModel(payload["document"] as never, source);
  // The schema travels with the draft rather than being fetched. An earlier
  // version pulled `/schemas/frontmatter.json`, which assumes whoever serves the
  // bundle also serves the repository's `schemas/` directory — the e2e server
  // does not, so the form silently never drew and only clicking the button
  // showed it. The server that has the document has the schema too.
  open = {
    model: state.model,
    source,
    schema: isRecord(payload["schema"]) ? (payload["schema"] as never) : null,
  };
  drawDraft();
}

/** Everything that depends on the open draft, drawn once. */
function drawDraft(): void {
  if (!state.model) return;
  // `renderSurface` emits the `[data-editor-surface]` element itself, and the
  // shell ships one so the page is not blank before the bundle runs. Replacing
  // its *children* nested a surface inside a surface and every selector for it
  // then matched twice; the rendered one replaces it outright.
  const surface = document.querySelector("[data-editor-surface]");
  if (surface) {
    const holder = document.createElement("div");
    holder.innerHTML = String(renderSurface(state.model, unexpanded()));
    surface.replaceWith(...holder.childNodes);
  }
  // Deliberately *not* `renderToolbar` here. It emits its own
  // `<header class="toolbar">`, so mounting it inside the shell's header nests
  // one inside the other — the same mistake as the surface — and it predates the
  // guide controls, so replacing the header outright would delete the six
  // buttons that do work. It has been superseded by `index.html`; what is real
  // in it is the role-aware primary action, and that is applied here instead.
  const primary = primaryAction(state.grant);
  const action = document.querySelector("[data-action]");
  if (action) {
    action.setAttribute("data-action", primary.action);
    action.textContent = primary.label;
  }
  replace("[data-problems]", renderProblems(state.source, []));
  const status = document.querySelector("[data-draft-status]");
  if (status) status.textContent = state.path === "" ? "Draft open." : `Editing ${state.path}`;
  announce(`${state.path || "A draft"} opened.`);
}

/** Replaces a region's contents with a fragment. */
function replace(selector: string, fragment: Fragment): void {
  const region = document.querySelector(selector);
  if (!region) return;
  const holder = document.createElement("div");
  holder.innerHTML = String(fragment);
  region.replaceChildren(...holder.childNodes);
}

const TOUR_SEEN = "liyasa.editor.tourSeen";

/**
 * Whether the tour has been taken.
 *
 * In `localStorage`, which throws in a private window and returns nothing when
 * site data is cleared — so a failure here means the tour opens again, never that
 * the editor does not load.
 */
function restoreTourSeen(): void {
  try {
    if (globalThis.localStorage?.getItem(TOUR_SEEN) === "1") shell = { ...shell, tourSeen: true };
  } catch {
    // An unreadable store is a tour the author sees twice, and nothing worse.
  }
}

function rememberTourSeen(): void {
  try {
    globalThis.localStorage?.setItem(TOUR_SEEN, "1");
  } catch {
    // Same: not worth a diagnostic, and never worth throwing out of a click.
  }
}

function attributesOf(node: Element): Record<string, string> {
  const out: Record<string, string> = {};
  for (const attribute of node.attributes) out[attribute.name] = attribute.value;
  return out;
}

/**
 * ED-02's source mode.
 *
 * Both modes render from the same `SourceDocument`, so switching is a re-render
 * and cannot lose anything — which is ED-03(d), and is a property of the model
 * rather than of this function. What this owes is that the switch exists at all:
 * the button has been in the shell since the first commit with nothing bound to
 * it.
 */
function switchMode(): void {
  if (!state.model) {
    announce("There is no draft open to switch.");
    return;
  }
  state.mode = state.mode === "visual" ? "source" : "visual";
  const host = document.querySelector("[data-editor-surface], [data-source-mode]");
  if (!host) return;
  const holder = document.createElement("div");
  holder.innerHTML =
    state.mode === "source"
      ? String(renderSourceMode({ document: state.model.document, source: state.source, problemLines: [] }))
      : String(renderSurface(state.model, unexpanded()));
  host.replaceWith(...holder.childNodes);
  for (const button of document.querySelectorAll("[data-mode-switch]")) {
    button.textContent = state.mode === "visual" ? "Source" : "Visual";
  }
  announce(state.mode === "source" ? "Source mode." : "Visual mode.");
}

function onClick(event: MouseEvent): void {
  const target = event.target;
  if (!(target instanceof Element)) return;
  if (target.closest("[data-mode-switch]")) {
    event.preventDefault();
    switchMode();
    return;
  }
  // `closest` rather than the target itself: the control may be a `<strong>`
  // inside the button, which is what a template choice is.
  const control = target.closest("button, [data-choose-template], [data-choose-task]");
  if (!control) return;
  const action = actionFor(attributesOf(control));
  if (!action) return;
  event.preventDefault();
  markOpener(control, action);
  apply(action);
}

function onKey(event: KeyboardEvent): void {
  if (event.key.toLowerCase() === "e" && event.ctrlKey && !event.altKey && !event.metaKey) {
    // `renderToolbar` has advertised `aria-keyshortcuts="Control+E"` all along,
    // which is a promise the shell had not kept.
    event.preventDefault();
    switchMode();
    return;
  }
  if (event.key === "Escape" && shell.panel.kind !== "none") {
    event.preventDefault();
    apply({ do: "close" });
    return;
  }
  // `?` opens help, unless the author is typing one into their page.
  if (event.key === "?" && !typing(event.target)) {
    event.preventDefault();
    apply({ do: "open-help" });
  }
}

/**
 * What the author is looking at, from what they have focused.
 *
 * This is the whole of "contextual" in ED-74's contextual help. Without it
 * `reduce` still handles a `context` action and nothing ever sends one, so the
 * help opens on whatever the shell started with — which is help, and is not
 * contextual help.
 *
 * Focus rather than the mouse: it follows the keyboard, it follows a click, and
 * it is the thing a screen reader user moves. Panes inside the panel column are
 * skipped, or opening the help would re-point the help at the help.
 */
function onFocus(event: FocusEvent): void {
  const target = event.target;
  if (!(target instanceof Element)) return;
  if (target.closest("[data-panel]")) return;
  const owner = target.closest("[data-help-context]");
  const topic = owner?.getAttribute("data-help-context");
  if (topic && topic !== shell.context) apply({ do: "context", topic });
}

function typing(target: EventTarget | null): boolean {
  if (!(target instanceof Element)) return false;
  return (
    target instanceof HTMLTextAreaElement ||
    target instanceof HTMLInputElement ||
    target.getAttribute("contenteditable") === "true"
  );
}

/**
 * Remembers which control opened a panel, so closing gives focus back to it.
 *
 * WCAG 2.4.3: a dialog that returns focus to the document loses a keyboard user
 * their place, and it is the commonest way an otherwise accessible panel fails.
 */
function markOpener(control: Element, action: Action): void {
  if (action.do === "close") return;
  for (const previous of document.querySelectorAll("[data-panel-opener]")) {
    previous.removeAttribute("data-panel-opener");
  }
  if (control.closest("[data-panel]") === null) control.setAttribute("data-panel-opener", "");
}

function apply(action: Action): void {
  // The whole panel, not just its `kind`. Comparing kinds missed every change
  // *within* one: advancing the tour from step 1 to step 2 stays `"tour"`, so
  // the step changed on screen and a screen reader was told nothing. The same
  // held for choosing a template and for moving the properties pane to another
  // block — three silent changes from one comparison at the wrong granularity.
  const before = JSON.stringify(shell.panel);
  shell = reduce(shell, action);
  if (shell.tourSeen) rememberTourSeen();
  paint();
  if (JSON.stringify(shell.panel) !== before) announce(announcementFor(shell, open));
  const pressed = document.querySelector("[data-advanced]");
  pressed?.setAttribute("aria-pressed", shell.advanced ? "true" : "false");
}

function paint(): void {
  const region = document.querySelector("[data-panel]");
  if (region) region.innerHTML = "";
  for (const stale of document.querySelectorAll("[data-tour-step]")) stale.remove();
  if (shell.panel.kind === "none") {
    focusOn(shell.focus);
    return;
  }
  const host = document.querySelector(landmarkFor(shell.panel) ?? "[data-panel]");
  if (!host) return;
  const holder = document.createElement("div");
  holder.innerHTML = String(renderPanel(shell, open));
  // The tour goes beside the shell rather than inside the panel column, because
  // a step points at something on screen and cannot sit inside what it points at.
  host.append(...holder.childNodes);
  focusOn(shell.focus);
}

function focusOn(selector: string | null): void {
  if (selector === null) return;
  const target = document.querySelector(selector);
  if (target instanceof HTMLElement) target.focus();
}

function announce(message: string): void {
  state.announcement = message;
  const region = document.querySelector("[data-announce]");
  if (region) region.textContent = message;
}

if (typeof document !== "undefined") {
  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", mount, { once: true });
  } else {
    mount();
  }
}

// Every module this application is made of is reachable from the entry, so the
// bundler walks the whole graph and the budget covers all of it. Listing them
// here rather than calling each one is deliberate: the shell wires them up as
// the panes are built, and a bundle that dropped half the editor because the
// shell had not yet reached it would pass its own budget test.
export const MODULES = {
  buildModel,
  editBlock,
  editBlocks,
  serializeModel,
  tokenize,
  decorate,
  placeDiagnostics,
  SLASH_COMMANDS,
  slashInsert,
  slashMatches,
  matchShortcut,
  moveBlock,
  htmlToMarkdown,
  imagesIn,
  pasteSource,
  PREVIEW_DEFAULTS,
  previewLimits,
  previewContext,
  capIterations,
  capBytes,
  chipSource,
  parseFrontmatter,
  writeFrontmatter,
  formFields,
  validateFrontmatter,
  routeOf,
  navigationOf,
  createPage,
  renamePage,
  movePage,
  duplicatePage,
  deletePage,
  reorderNavigation,
  findReplace,
  applyPlan,
  changeFactReference,
  moveGroup,
  applyTag,
  sniff,
  validateUpload,
  imageDirective,
  darkPartner,
  searchAssets,
  deleteRefusal,
  transformQuery,
  draftBranch,
  searchDrafts,
  pagesTouched,
  ageOf,
  save,
  merge3,
  seenTab,
  alsoOpenElsewhere,
  call,
  endpointPath,
  findEndpoint,
  unservedBy,
  may,
  refusal,
  primaryAction,
  parseDocowners,
  ownersFor,
  assignReviewers,
  staleReminders,
  queueRows,
  decide,
  publishPlan,
  OPERATIONS,
  runPayload,
  screen,
  setStatus,
  rejectAll,
  acceptedEdits,
  pendingCount,
  ACTIVITY_KINDS,
  KIND_LABEL,
  feed,
  byDay,
  counts,
  TASKS,
  suggestEditUrl,
  quickFixProposal,
  updateANumber,
  renameEverywhere,
  replaceAScreenshot,
  addAFaqEntry,
  recordAChangelogEntry,
  VOCABULARY,
  say,
  TOUR,
  TEMPLATES,
  pageFromTemplate,
  HELP,
  chartTable,
  saveAnnouncement,
  validationAnnouncement,
  reviewAnnouncement,
  messageFor,
  EditorSession,
  PreviewHold,
  sessionNonce,
  renderSurface,
  renderBlock,
  renderProblems,
  renderToolbar,
  renderShortcuts,
  renderFrontmatterForm,
  valueFromControl,
  renderProperties,
  formFor,
  byCategory,
  propFromControl,
  propsToDirective,
  renderSourceMode,
  renderSourcePopover,
  highlight,
  opaqueReason,
  renderSuggestions,
  renderSuggestion,
  suggestionAnnouncement,
  renderDrafts,
  renderConflict,
  renderActivity,
  renderMedia,
  renderEmpty,
  renderContextToolbar,
  describeContext,
  renderCappedRows,
  renderVocabulary,
  renderTourStep,
  renderHelp,
  termsIn,
  renderTemplatePicker,
  renderTaskList,
  renderTaskForm,
  askLabel,
  renderProposal,
  renderPanel,
  reduce,
  actionFor,
  announcementFor,
  landmarkFor,
  renderNode,
  renderChip,
  chipTooltip,
  renderLogicBlock,
  renderExpressionEditor,
  expressionCompletions,
  renderComponent,
  renderCode,
  renderOpaque,
  unexpanded,
  announce,
  state,
};
