import { test } from "node:test";
import assert from "node:assert/strict";

import { SELECTOR, bind } from "../src/shortcut.ts";
import { element, vendoredWindow } from "./dom.ts";
import type { FakeElement, VendoredWindow } from "./dom.ts";

interface Press {
  key: string;
  ctrlKey?: boolean;
  metaKey?: boolean;
  shiftKey?: boolean;
  altKey?: boolean;
}

function copyButton(): FakeElement {
  const button = element("button", { "data-ly-action": "copy-markdown" });
  // `copy.js` reveals the button once it has wired a handler to it.
  button.hidden = false;
  return button;
}

function load(button: FakeElement | null = copyButton()): VendoredWindow {
  const win = vendoredWindow();
  win.query.set(SELECTOR, button ? [button] : []);
  assert.equal(bind(win as never), button !== null);
  return win;
}

function press(win: VendoredWindow, event: Press): boolean {
  let prevented = false;
  win.document.dispatch("keydown", {
    ctrlKey: false,
    metaKey: false,
    shiftKey: false,
    altKey: false,
    ...event,
    preventDefault() {
      prevented = true;
    },
  });
  return prevented;
}

test("control-shift-c copies the page as markdown", () => {
  const button = copyButton();
  const win = load(button);
  assert.equal(press(win, { key: "C", ctrlKey: true, shiftKey: true }), true);
  assert.equal(button.clicks, 1);
});

test("the mac half of mod is the same shortcut", () => {
  const button = copyButton();
  const win = load(button);
  assert.equal(press(win, { key: "C", metaKey: true, shiftKey: true }), true);
  assert.equal(button.clicks, 1);
});

test("a keyboard that reports the unshifted key still matches", () => {
  const button = copyButton();
  const win = load(button);
  press(win, { key: "c", ctrlKey: true, shiftKey: true });
  assert.equal(button.clicks, 1);
});

test("the reader's own copy is left to the browser", () => {
  const button = copyButton();
  const win = load(button);
  assert.equal(press(win, { key: "c", ctrlKey: true }), false);
  assert.equal(button.clicks, 0);
});

test("shift-c without a modifier is a capital C", () => {
  const button = copyButton();
  const win = load(button);
  assert.equal(press(win, { key: "C", shiftKey: true }), false);
  assert.equal(button.clicks, 0);
});

test("a chord that adds alt is someone else's", () => {
  const button = copyButton();
  const win = load(button);
  assert.equal(press(win, { key: "C", ctrlKey: true, shiftKey: true, altKey: true }), false);
  assert.equal(button.clicks, 0);
});

test("a page whose menu excludes the action binds nothing", () => {
  const win = load(null);
  assert.deepEqual(Object.keys(win.document.listeners), []);
});

test("a button copy.js never revealed is not clicked", () => {
  const button = copyButton();
  button.hidden = true;
  const win = load(button);
  assert.equal(press(win, { key: "C", ctrlKey: true, shiftKey: true }), false);
  assert.equal(button.clicks, 0);
});
