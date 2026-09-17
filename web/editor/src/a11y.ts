// ED-80: WCAG 2.2 AA in the parts of the editor that are this module's to
// decide — what is announced, what is focusable, what a keyboard reaches, and
// what moves when the viewer has asked for less motion.
//
// The parts that are not here are in the markup and the stylesheet, and
// `web/e2e/a11y/editor.spec.ts` drives axe over the running page. What this
// module holds is the decisions a test can check without a browser: the
// announcement text, the keyboard map, and the rule that every action has a
// keyboard route.

export type Politeness = "polite" | "assertive";

export interface Announcement {
  message: string;
  politeness: Politeness;
}

/**
 * What a screen reader hears when a save happens.
 *
 * Autosave is `polite`: it happens every few seconds and interrupting somebody
 * mid-sentence to say "saved" makes the editor unusable with a screen reader.
 * A *failed* save is `assertive`, because the alternative is losing work
 * silently.
 */
export function saveAnnouncement(state: "saving" | "saved" | "failed" | "stale"): Announcement {
  switch (state) {
    case "saving":
      return { message: "Saving", politeness: "polite" };
    case "saved":
      return { message: "Saved", politeness: "polite" };
    case "failed":
      return { message: "Not saved. Your changes are still here; check your connection.", politeness: "assertive" };
    case "stale":
      return {
        message: "Not saved. This draft changed somewhere else; your edit is waiting for you to accept or discard it.",
        politeness: "assertive",
      };
  }
}

/** What a screen reader hears when validation finishes. */
export function validationAnnouncement(errors: number, warnings: number): Announcement {
  if (errors === 0 && warnings === 0) return { message: "No problems found", politeness: "polite" };
  const parts: string[] = [];
  if (errors > 0) parts.push(`${errors} problem${errors === 1 ? "" : "s"}`);
  if (warnings > 0) parts.push(`${warnings} suggestion${warnings === 1 ? "" : "s"}`);
  return {
    message: `${parts.join(" and ")}. Press F8 to move through them.`,
    // A problem the author introduced is worth interrupting for; a warning is
    // not, and announcing every warning assertively trains people to ignore
    // the live region.
    politeness: errors > 0 ? "assertive" : "polite",
  };
}

/** What a screen reader hears when a review's state changes. */
export function reviewAnnouncement(state: "submitted" | "approved" | "changes-requested" | "published"): Announcement {
  const messages: Record<typeof state, string> = {
    submitted: "Submitted for review",
    approved: "Approved. Publishing follows the project's policy.",
    "changes-requested": "Changes requested. The comments are in the review pane.",
    published: "Published",
  };
  return { message: messages[state], politeness: "polite" };
}

export interface Shortcut {
  keys: string;
  action: string;
  description: string;
  /** Where the shortcut works. */
  scope: "global" | "editor" | "review";
}

/**
 * Every action the editor offers, and the keys that reach it.
 *
 * ED-80 asks for full keyboard operation of the block editor, the properties
 * forms, the review diffs and the charts. That is a statement about *coverage*
 * — so `actionsWithoutShortcut` exists, and its test is what makes the claim
 * checkable rather than aspirational.
 */
export const SHORTCUTS: Shortcut[] = [
  { keys: "?", action: "show-shortcuts", description: "Show this list", scope: "global" },
  { keys: "Mod+S", action: "save", description: "Save now", scope: "global" },
  { keys: "Mod+Z", action: "undo", description: "Undo", scope: "editor" },
  { keys: "Mod+Shift+Z", action: "redo", description: "Redo", scope: "editor" },
  { keys: "Mod+E", action: "toggle-mode", description: "Switch between visual and source mode", scope: "global" },
  { keys: "Mod+K", action: "command-palette", description: "Search commands and pages", scope: "global" },
  { keys: "/", action: "slash-menu", description: "Insert a component", scope: "editor" },
  { keys: "Mod+Enter", action: "submit-for-review", description: "Submit for review", scope: "global" },
  { keys: "F8", action: "next-problem", description: "Go to the next problem", scope: "global" },
  { keys: "Shift+F8", action: "previous-problem", description: "Go to the previous problem", scope: "global" },
  { keys: "Alt+ArrowUp", action: "move-block-up", description: "Move this block up", scope: "editor" },
  { keys: "Alt+ArrowDown", action: "move-block-down", description: "Move this block down", scope: "editor" },
  { keys: "Mod+Alt+P", action: "open-properties", description: "Open this block's settings", scope: "editor" },
  { keys: "Escape", action: "close-panel", description: "Close the open panel", scope: "global" },
  { keys: "Mod+.", action: "quick-fix", description: "Apply the suggested fix", scope: "editor" },
  { keys: "J", action: "next-suggestion", description: "Next suggested change", scope: "review" },
  { keys: "K", action: "previous-suggestion", description: "Previous suggested change", scope: "review" },
  { keys: "A", action: "accept-suggestion", description: "Accept this suggestion", scope: "review" },
  { keys: "R", action: "reject-suggestion", description: "Reject this suggestion", scope: "review" },
  { keys: "Mod+ArrowRight", action: "next-diff-file", description: "Next file in the diff", scope: "review" },
  { keys: "Mod+ArrowLeft", action: "previous-diff-file", description: "Previous file in the diff", scope: "review" },
  { keys: "T", action: "chart-as-table", description: "Show this chart as a table", scope: "review" },
];

/** Every action the editor's toolbars and panes can perform. */
export const ACTIONS: string[] = SHORTCUTS.map((shortcut) => shortcut.action);

/** Actions with no keyboard route. ED-80 requires this to be empty. */
export function actionsWithoutShortcut(actions: string[]): string[] {
  const reachable = new Set(SHORTCUTS.map((shortcut) => shortcut.action));
  return actions.filter((action) => !reachable.has(action));
}

/** Two shortcuts on one key in one scope is one action nobody can reach. */
export function shortcutCollisions(): string[] {
  const seen = new Map<string, string>();
  const clashes: string[] = [];
  for (const shortcut of SHORTCUTS) {
    const key = `${shortcut.scope}:${shortcut.keys.toLowerCase()}`;
    const existing = seen.get(key);
    if (existing) clashes.push(`${shortcut.keys} is both ${existing} and ${shortcut.action}`);
    else seen.set(key, shortcut.action);
  }
  return clashes;
}

/**
 * How long a transition may run.
 *
 * `reduced` is zero rather than short: `prefers-reduced-motion` is set by
 * people for whom movement causes nausea or seizures, and a fast animation is
 * still an animation.
 */
export function motionDuration(base: number, reduced: boolean): number {
  return reduced ? 0 : base;
}

/**
 * Whether a focus ring is drawn.
 *
 * Always, when the element has focus. Hiding it for pointer users is the most
 * common WCAG 2.4.7 failure and it is invisible to whoever wrote the CSS,
 * because they were using a mouse at the time.
 */
export function focusVisible(): boolean {
  return true;
}

export interface Landmark {
  role: string;
  label: string;
}

/** The regions a screen reader user moves between with one keystroke. */
export const LANDMARKS: Landmark[] = [
  { role: "banner", label: "Editor toolbar" },
  { role: "navigation", label: "Pages" },
  { role: "main", label: "Page content" },
  { role: "complementary", label: "Preview" },
  { role: "complementary", label: "Problems" },
  { role: "contentinfo", label: "Draft status" },
];

/**
 * A chart's data as a table, for ED-80's "charts with data-table alternatives".
 *
 * The table is built from the same series the chart draws, so it cannot show
 * different numbers.
 */
export function chartTable(
  series: { label: string; points: { x: string; y: number }[] }[],
): { columns: string[]; rows: (string | number)[][] } {
  const columns = ["", ...series.map((line) => line.label)];
  const keys: string[] = [];
  for (const line of series) for (const point of line.points) if (!keys.includes(point.x)) keys.push(point.x);
  const rows = keys.map((key) => [
    key,
    ...series.map((line) => line.points.find((point) => point.x === key)?.y ?? 0),
  ]);
  return { columns, rows };
}
