// What the shell shows, as a value rather than as a pile of event handlers.
//
// `editor.ts` is the only module that touches the document, and nothing in it
// runs under `node --test`. That is fine for three lines of mounting and wrong
// for the panels: which panel a click opens, what closes it, where it goes and
// what a screen reader hears are all decisions, and a decision that lives inside
// an event listener is a decision no test can reach.
//
// So the panels are a small machine. `reduce` maps an action to the next state,
// `renderPanel` draws it, `landmarkFor` says which region it belongs in and
// `announcementFor` says what is announced. `editor.ts` keeps one delegated
// listener that turns a `data-*` attribute into an `Action` and applies the
// result — no branching of its own.
//
// Only the panels that need nothing from the network are here. A draft, a
// preview and a suggestion all wait on `/_liyasa/editor/`, which does not exist;
// the tour, the help, the templates, the task list and the vocabulary do not,
// and an author can use every one of them today.

import { html } from "../escape.ts";
import type { Fragment } from "../escape.ts";
import { TOUR } from "../help.ts";
import { renderHelp, renderTaskForm, renderTaskList, renderTemplatePicker, renderTourStep, renderVocabulary } from "./guides.ts";
import { renderProperties } from "./properties.ts";
import { renderSourcePopover, opaqueReason } from "./source-mode.ts";
import { renderFrontmatterForm } from "./form.ts";
import { formFields, parseFrontmatter, validateFrontmatter } from "../frontmatter.ts";
import { flatten } from "../model.ts";
import type { EditorModel, EditorNode } from "../model.ts";

/** Which panel the shell is showing, and its own state. */
export type Panel =
  | { kind: "none" }
  | { kind: "help"; topic: string }
  | { kind: "tour"; step: number }
  | { kind: "templates"; chosen?: string }
  | { kind: "tasks" }
  | { kind: "task"; id: string }
  | { kind: "vocabulary" }
  // The four below need the open draft to draw. They carry the id of what the
  // author acted on, never the data — `reduce` stays pure and the draft is
  // passed to `renderPanel` instead, so the machine cannot go stale against it.
  | { kind: "properties"; block: string }
  | { kind: "source"; block: string }
  | { kind: "frontmatter" };

export interface ShellState {
  panel: Panel;
  /** What the author is looking at, which is what "contextual help" means. */
  context: string;
  advanced: boolean;
  /** Set once the tour is finished or skipped, so it does not reopen. */
  tourSeen: boolean;
  /** Where focus goes after the panel changes. */
  focus: string | null;
}

export const START: ShellState = {
  panel: { kind: "none" },
  context: "frontmatter",
  advanced: false,
  tourSeen: false,
  focus: null,
};

/** Everything a control in the shell can ask for. */
export type Action =
  | { do: "open-help"; topic?: string }
  | { do: "open-tour" }
  | { do: "tour-next" }
  | { do: "tour-back" }
  | { do: "tour-skip" }
  | { do: "tour-done" }
  | { do: "open-templates" }
  | { do: "choose-template"; id: string }
  | { do: "open-tasks" }
  | { do: "choose-task"; id: string }
  | { do: "cancel-task" }
  | { do: "open-vocabulary" }
  | { do: "toggle-advanced" }
  | { do: "close" }
  | { do: "first-visit"; seen: boolean }
  | { do: "context"; topic: string }
  | { do: "open-properties"; block: string }
  | { do: "edit-source"; block: string }
  | { do: "open-frontmatter" };

/**
 * The next state.
 *
 * Two rules worth naming because both were bugs in an earlier draft of this.
 *
 * Opening a panel that is already open **closes** it, because the control that
 * opens it is the same button, and a button whose only effect is to re-render
 * what is already there is a button that appears broken.
 *
 * `focus` is set on every transition that changes the panel, and set to the
 * control that *opened* it when the panel closes. A panel that takes focus and
 * gives it back to `document.body` loses a keyboard user their place, which is
 * WCAG 2.4.3 and the most common way an otherwise accessible dialog fails.
 */
export function reduce(state: ShellState, action: Action): ShellState {
  switch (action.do) {
    case "open-help": {
      const topic = action.topic ?? state.context;
      return state.panel.kind === "help" && state.panel.topic === topic
        ? closed(state)
        : { ...state, panel: { kind: "help", topic }, focus: "[data-help-topic]" };
    }
    case "open-tour":
      return { ...state, panel: { kind: "tour", step: 0 }, focus: "[data-tour-next]" };
    case "tour-next": {
      if (state.panel.kind !== "tour") return state;
      const next = state.panel.step + 1;
      // The last step's control is Finish, not Next, so this cannot run past the
      // end — but a keyboard shortcut could, and a tour that renders nothing
      // looks like a crash.
      if (next >= TOUR.length) return { ...closed(state), tourSeen: true };
      return { ...state, panel: { kind: "tour", step: next }, focus: "[data-tour-next]" };
    }
    case "tour-back":
      return state.panel.kind === "tour" && state.panel.step > 0
        ? { ...state, panel: { kind: "tour", step: state.panel.step - 1 }, focus: "[data-tour-next]" }
        : state;
    case "tour-skip":
    case "tour-done":
      return { ...closed(state), tourSeen: true };
    case "open-templates":
      return state.panel.kind === "templates"
        ? closed(state)
        : { ...state, panel: { kind: "templates" }, focus: "[data-choose-template]" };
    case "choose-template":
      return { ...state, panel: { kind: "templates", chosen: action.id }, focus: "[data-choose-template]" };
    case "open-tasks":
      return state.panel.kind === "tasks"
        ? closed(state)
        : { ...state, panel: { kind: "tasks" }, focus: "[data-choose-task]" };
    case "choose-task":
      return { ...state, panel: { kind: "task", id: action.id }, focus: "[data-task-field]" };
    case "cancel-task":
      return { ...state, panel: { kind: "tasks" }, focus: "[data-choose-task]" };
    case "open-vocabulary":
      return state.panel.kind === "vocabulary"
        ? closed(state)
        : { ...state, panel: { kind: "vocabulary" }, focus: "[data-vocabulary] summary" };
    case "toggle-advanced":
      return { ...state, advanced: !state.advanced };
    case "close":
      return closed(state);
    case "first-visit":
      // The tour opens itself once, and only when nothing else is open: a tour
      // that appears over a panel the author opened on purpose is an
      // interruption, not an introduction.
      return action.seen || state.tourSeen || state.panel.kind !== "none"
        ? { ...state, tourSeen: action.seen || state.tourSeen }
        : { ...state, panel: { kind: "tour", step: 0 }, focus: "[data-tour-next]" };
    case "open-properties":
      // Re-pressing the same block's handle closes, as everywhere else; pressing
      // a *different* block's swaps rather than closing, because the author is
      // moving along the page rather than dismissing a panel.
      return state.panel.kind === "properties" && state.panel.block === action.block
        ? closed(state)
        : { ...state, panel: { kind: "properties", block: action.block }, focus: "[data-prop]" };
    case "edit-source":
      return state.panel.kind === "source" && state.panel.block === action.block
        ? closed(state)
        : { ...state, panel: { kind: "source", block: action.block }, focus: "[data-source-popover] [data-source-editor]" };
    case "open-frontmatter":
      return state.panel.kind === "frontmatter"
        ? closed(state)
        : { ...state, panel: { kind: "frontmatter" }, focus: "[data-frontmatter] input, [data-frontmatter] select, [data-frontmatter] textarea" };
    case "context":
      // Changing what the author is looking at re-points open help at it, and
      // leaves every other panel alone.
      //
      // `focus: null` is the whole of this case's difficulty. Every other action
      // is a control being pressed, so moving focus is right; this one is
      // dispatched *by* a focus move, and a state that carried the previous
      // action's `focus` selector sent the author straight back out of whatever
      // they had just reached. Tabbing into a chip threw focus to the panel, so
      // the surface after the first help-bearing block was unreachable by
      // keyboard — WCAG 2.4.3, found by the accessibility suite rather than by
      // reading this.
      return state.panel.kind === "help"
        ? { ...state, context: action.topic, panel: { kind: "help", topic: action.topic }, focus: null }
        : { ...state, context: action.topic, focus: null };
  }
}

function closed(state: ShellState): ShellState {
  return { ...state, panel: { kind: "none" }, focus: "[data-panel-opener]" };
}

/**
 * What the shell knows about the draft on screen.
 *
 * Passed to `renderPanel` rather than held in `ShellState`, so the machine has
 * nothing to keep in step with the document and `reduce` stays a pure function
 * of actions. `null` is the state the editor is in until a draft arrives.
 */
export interface OpenDraft {
  model: EditorModel;
  source: string;
  /** `schemas/frontmatter.json`, fetched once; `null` until it has loaded. */
  schema: Parameters<typeof formFields>[0] | null;
}

/** The node an action named, or nothing when the draft does not have it. */
export function blockIn(open: OpenDraft | null, id: string): EditorNode | undefined {
  if (!open) return undefined;
  const found = flatten(open.model).find((each) => "kind" in each && each.id === id);
  return found as EditorNode | undefined;
}

/** The panel, drawn. */
export function renderPanel(state: ShellState, open: OpenDraft | null = null): Fragment {
  switch (state.panel.kind) {
    case "none":
      return html``;
    case "help":
      return renderHelp(state.panel.topic, state.advanced);
    case "tour":
      return renderTourStep(state.panel.step);
    case "templates":
      return renderTemplatePicker(state.panel.chosen);
    case "tasks":
      return renderTaskList();
    case "task":
      return renderTaskForm(state.panel.id);
    case "vocabulary":
      return renderVocabulary(true);
    case "properties": {
      const node = blockIn(open, state.panel.block);
      if (!node) return noDraft("the properties of a block", state.panel.block);
      return renderProperties({
        component: node.name ?? node.kind,
        props: node.props ?? {},
        block: node.id,
      });
    }
    case "source": {
      const node = blockIn(open, state.panel.block);
      if (!node) return noDraft("the source of a block", state.panel.block);
      return renderSourcePopover({
        block: node.id,
        text: node.text,
        reason: opaqueReason(node.kind, node.text),
      });
    }
    case "frontmatter": {
      if (!open) return noDraft("the page settings", "frontmatter");
      if (!open.schema) return noSchema();
      const values = parseFrontmatter(open.model.frontmatter).fields;
      return renderFrontmatterForm({
        fields: formFields(open.schema),
        values,
        validation: validateFrontmatter(open.schema, values),
        advanced: state.advanced,
      });
    }
  }
}

/**
 * What a panel says when the draft it needs is not there.
 *
 * The same distinction `panes.ts` draws: this is not an empty form, it is an
 * unknown one. A blank properties panel would read as "this component has no
 * properties", which is a claim about the component rather than about the build.
 */
function noDraft(what: string, id: string): Fragment {
  return html`<p class="empty unserved" data-unserved="ED-20" data-wanted="${id}">
    Nothing has opened a draft in this build yet, so ${what} is not available. The
    editor's draft route is not served.
  </p>`;
}

/** The schema is fetched, so it can be absent for a moment or for good. */
function noSchema(): Fragment {
  return html`<p class="empty unserved" data-unserved="ED-11">
    The page settings form is generated from the project's schema, and the schema
    has not loaded. Nothing is shown rather than a form with no rules behind it.
  </p>`;
}

/**
 * Which landmark a panel belongs in.
 *
 * The tour is the exception and it is the whole reason this is a function: a
 * tour step points at something on screen, so it cannot be inside the thing it
 * points at. Everything else goes in the side column, beside the page rather
 * than over it.
 */
export function landmarkFor(panel: Panel): string | null {
  switch (panel.kind) {
    case "none":
      return null;
    case "tour":
      return "[data-editor]";
    case "source":
      // ED-03(c) calls it a popover: it belongs over the block whose bytes it
      // shows, not in a column somewhere else on the page.
      return `[data-block="${panel.block}"]`;
    default:
      return "[data-panel]";
  }
}

/**
 * What a screen reader hears when the panel changes.
 *
 * A panel that appears silently is a panel a screen reader user does not know
 * about: the visual change is the announcement for everybody else.
 */
export function announcementFor(state: ShellState, open: OpenDraft | null = null): string {
  switch (state.panel.kind) {
    case "none":
      return "Closed.";
    case "help":
      return "Help opened.";
    case "tour":
      return `Tour step ${state.panel.step + 1} of ${TOUR.length}.`;
    case "templates":
      return state.panel.chosen === undefined ? "Templates opened." : `${state.panel.chosen} template chosen.`;
    case "tasks":
      return "Tasks opened.";
    case "task":
      return `${state.panel.id} opened. Nothing changes until you submit it.`;
    case "vocabulary":
      return "Word list opened.";
    case "properties": {
      // Named, because moving the pane from one block to the next is a change a
      // screen reader is otherwise told nothing about: "Properties opened."
      // twice in a row says the same thing about two different blocks.
      const node = blockIn(open, state.panel.block);
      return node ? `Properties for ${node.name ?? node.kind}.` : "Properties opened.";
    }
    case "source":
      return "Editing this block as source.";
    case "frontmatter":
      return "Page settings opened.";
  }
}

/**
 * The action a clicked element asks for, or nothing.
 *
 * A lookup rather than a chain of `if`s, and it takes the attribute names the
 * renderers already emit — so a control renamed in a view and not here stops
 * working, which the tests catch, rather than silently doing nothing.
 */
export function actionFor(attributes: Record<string, string>): Action | null {
  if ("data-help" in attributes) return { do: "open-help", topic: attributes["data-help"] || undefined };
  if ("data-open-tour" in attributes) return { do: "open-tour" };
  if ("data-tour-next" in attributes) return { do: "tour-next" };
  if ("data-tour-back" in attributes) return { do: "tour-back" };
  if ("data-tour-skip" in attributes) return { do: "tour-skip" };
  if ("data-tour-done" in attributes) return { do: "tour-done" };
  if ("data-open-templates" in attributes) return { do: "open-templates" };
  if ("data-choose-template" in attributes) return { do: "choose-template", id: attributes["data-choose-template"] ?? "" };
  if ("data-open-tasks" in attributes) return { do: "open-tasks" };
  if ("data-choose-task" in attributes) return { do: "choose-task", id: attributes["data-choose-task"] ?? "" };
  if ("data-cancel-task" in attributes) return { do: "cancel-task" };
  if ("data-open-vocabulary" in attributes) return { do: "open-vocabulary" };
  if ("data-advanced" in attributes) return { do: "toggle-advanced" };
  if ("data-open-properties" in attributes) {
    return { do: "open-properties", block: attributes["data-open-properties"] ?? "" };
  }
  if ("data-edit-source" in attributes) return { do: "edit-source", block: attributes["data-edit-source"] ?? "" };
  if ("data-open-frontmatter" in attributes) return { do: "open-frontmatter" };
  if ("data-close-panel" in attributes) return { do: "close" };
  return null;
}
