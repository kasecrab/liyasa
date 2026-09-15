// RX-62: `mod+shift+c` copies the page as Markdown.
//
// The action itself is the theme's — `crates/liyasa-theme/src/actions.rs`
// resolves it and `assets/js/copy.js` fetches the `.md` twin, writes the
// clipboard, and announces the result. This module only reaches the same
// button a pointer would, so the shortcut and the menu item cannot drift
// (`plan/rfcs/1103-copy-markdown-shortcut.md`).

export const SELECTOR = '[data-ly-action="copy-markdown"]';

interface ShortcutEvent {
  key?: string;
  ctrlKey?: boolean;
  metaKey?: boolean;
  shiftKey?: boolean;
  altKey?: boolean;
  preventDefault?(): void;
}

interface ButtonLike {
  hidden: boolean;
  click(): void;
}

interface ShortcutDocument {
  querySelector(selector: string): ButtonLike | null;
  addEventListener(type: string, listener: (event: unknown) => void): void;
}

export interface ShortcutWindow {
  document: ShortcutDocument;
}

// TODO(rfc-1103): read the chord from an `accelerator` on the action once
// `liyasa_theme::actions::Action` carries one, so the menu can print the hint.
export function chord(event: ShortcutEvent): boolean {
  if (event.altKey === true) return false;
  if (event.shiftKey !== true) return false;
  if (event.ctrlKey !== true && event.metaKey !== true) return false;
  // Shift makes the key "C" on most layouts and "c" where it does not.
  return String(event.key ?? "").toLowerCase() === "c";
}

export function bind(win: ShortcutWindow): boolean {
  const doc = win.document;
  if (doc.querySelector(SELECTOR) === null) return false;

  doc.addEventListener("keydown", (raw: unknown) => {
    const event = raw as ShortcutEvent;
    if (!chord(event)) return;
    // Looked up per press: the menu is re-rendered across a view transition,
    // and copy.js leaves the button hidden until it has a handler for it.
    const button = doc.querySelector(SELECTOR);
    if (button === null || button.hidden) return;
    event.preventDefault?.();
    button.click();
  });
  return true;
}
