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

/** Which panel the shell is showing, and its own state. */
export type Panel =
  | { kind: "none" }
  | { kind: "help"; topic: string }
  | { kind: "tour"; step: number }
  | { kind: "templates"; chosen?: string }
  | { kind: "tasks" }
  | { kind: "task"; id: string }
  | { kind: "vocabulary" };

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
  | { do: "context"; topic: string };

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
    case "context":
      // Changing what the author is looking at re-points open help at it, and
      // leaves every other panel alone.
      return state.panel.kind === "help"
        ? { ...state, context: action.topic, panel: { kind: "help", topic: action.topic } }
        : { ...state, context: action.topic };
  }
}

function closed(state: ShellState): ShellState {
  return { ...state, panel: { kind: "none" }, focus: "[data-panel-opener]" };
}

/** The panel, drawn. */
export function renderPanel(state: ShellState): Fragment {
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
  }
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
export function announcementFor(state: ShellState): string {
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
  if ("data-close-panel" in attributes) return { do: "close" };
  return null;
}
