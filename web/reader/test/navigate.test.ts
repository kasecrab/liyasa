import { test } from "node:test";
import assert from "node:assert/strict";

import { RULE, install, supported } from "../src/navigate.ts";
import { fakeWindow } from "./dom.ts";

test("a navigation is declared to the browser, not driven by a router", () => {
  const win = fakeWindow();
  assert.equal(install(win as never), true);
  assert.equal(win.document.adoptedStyleSheets.length, 1);
  assert.equal(win.document.adoptedStyleSheets[0]?.text, RULE);
  assert.match(RULE, /@view-transition\s*\{\s*navigation:\s*auto;\s*\}/);

  // No click, popstate, or fetch handler: the browser performs the navigation
  // and the transition, which is what RX-04 asks for without a client router.
  assert.deepEqual(Object.keys(win.document.listeners), []);
  assert.deepEqual(Object.keys(win.listeners), []);
});

test("a browser without view transitions is left alone", () => {
  const win = fakeWindow({ viewTransitions: false });
  assert.equal(supported(win as never), false);
  assert.equal(install(win as never), false);
  assert.equal(win.document.adoptedStyleSheets.length, 0);
});

test("a browser without constructable stylesheets is left alone", () => {
  const win = fakeWindow({ constructableStyleSheets: false });
  assert.equal(install(win as never), false);
  assert.equal(win.document.adoptedStyleSheets.length, 0);
});

test("reduced motion means no transition", () => {
  const win = fakeWindow({ reducedMotion: true });
  assert.equal(install(win as never), true);
  assert.equal(win.document.adoptedStyleSheets.length, 0);
});

test("the transition follows the reader's motion setting as it changes", () => {
  const win = fakeWindow();
  install(win as never);
  const motion = win.media.get("(prefers-reduced-motion: reduce)");
  assert.ok(motion);
  motion.set(true);
  assert.equal(win.document.adoptedStyleSheets.length, 0);
  motion.set(false);
  assert.equal(win.document.adoptedStyleSheets.length, 1);
  motion.set(false);
  assert.equal(win.document.adoptedStyleSheets.length, 1, "the sheet is adopted once");
});

test("the stylesheet is adopted, never injected as a style element", () => {
  // `style-src 'self'` plus hashes (RX-110) blocks an injected <style>; a
  // constructed sheet is not subject to it.
  const win = fakeWindow();
  install(win as never);
  assert.equal(win.document.adoptedStyleSheets[0] instanceof (win.CSSStyleSheet as never), true);
});
