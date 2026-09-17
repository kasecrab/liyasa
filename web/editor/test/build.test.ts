// The bundle builds, fits its budget, the committed `dist/` matches, and the
// shell says what it should before any script runs.

import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { gzipSync } from "node:zlib";
import test from "node:test";
import assert from "node:assert/strict";

import { bundle } from "../../reader/build.mjs";
import { ENTRIES } from "../build.mjs";
import { LANDMARKS } from "../src/a11y.ts";

const HERE = dirname(fileURLToPath(import.meta.url));
const shell = readFileSync(resolve(HERE, "..", "index.html"), "utf8");
const css = readFileSync(resolve(HERE, "..", "editor.css"), "utf8");

test("the committed bundle is what the source produces", () => {
  for (const { entry, out, budget } of ENTRIES) {
    const built = bundle(resolve(HERE, "..", entry), (path: string) => readFileSync(path, "utf8"));
    const committed = readFileSync(resolve(HERE, "..", "dist", out), "utf8");
    assert.equal(built, committed, `${out} is stale: run \`npm run build:editor\``);
    const compressed = gzipSync(Buffer.from(built), { level: 9 }).length;
    assert.ok(compressed <= budget, `${out}: ${compressed} bytes compressed, budget ${budget}`);
  }
});

test("the bundle touches the document in one place", () => {
  // Rendering is pure and `editor.ts` is the only module that mounts. That is
  // what makes `test/` need no browser: a test against a stand-in DOM proves
  // the stand-in works.
  const built = readFileSync(resolve(HERE, "..", "dist", "editor.js"), "utf8");
  const mounts = built.match(/document\.querySelector\("\[data-editor\]"\)/g) ?? [];
  assert.equal(mounts.length, 1);
});

test("ED-80: every landmark in the keyboard map is in the shell", () => {
  for (const landmark of LANDMARKS) {
    assert.ok(shell.includes(`data-landmark="${landmark.label}"`), `${landmark.label} is in the shell`);
  }
});

test("ED-80: the shell has one main, one live region and a skip target", () => {
  assert.equal((shell.match(/<main\b/g) ?? []).length, 1);
  assert.match(shell, /data-announce[^>]*aria-live="polite"/);
  assert.match(shell, /id="main"/);
});

test("the shell says what happened when the bundle does not run", () => {
  // "An application may assume it runs" is not "render a blank page while it
  // loads". A dropped bundle should leave a sentence, not a white rectangle.
  assert.match(shell, /<noscript>/);
  assert.match(shell, /needs JavaScript/);
  assert.match(shell, /repository/, "and a way to make the change without it");
});

test("the shell is not indexed", () => {
  assert.match(shell, /name="robots" content="noindex"/);
});

test("no rule outside the fallback blocks names a literal colour", () => {
  // A second palette is a second dark mode to maintain, and the theme's is the
  // one that has been checked for contrast.
  const start = css.indexOf("body {");
  assert.ok(start > 0, "the fallback blocks come first and `body` follows them");
  const literals = css.slice(start).match(/#[0-9a-fA-F]{3,8}\b|rgba?\(|hsla?\(/g) ?? [];
  assert.deepEqual(literals, []);
});

test("ED-80: the focus ring is defined and never removed", () => {
  assert.match(css, /:focus-visible\s*\{[^}]*outline:/);
  assert.ok(!/outline:\s*(0|none)/.test(css), "nothing in this file takes the ring away");
});

test("ED-80: reduced motion is honoured", () => {
  assert.match(css, /@media \(prefers-reduced-motion: reduce\)/);
  assert.match(css, /transition-duration: 0\.01ms !important/);
});

test("the live region is hidden without being silenced", () => {
  // `display: none` and `visibility: hidden` both take an element out of the
  // accessibility tree, which is the one thing a live region must stay in.
  const block = css.slice(css.indexOf("[data-announce]"));
  const rule = block.slice(0, block.indexOf("}"));
  assert.ok(!/display:\s*none/.test(rule));
  assert.ok(!/visibility:\s*hidden/.test(rule));
  assert.match(rule, /clip-path/);
});

test("the shell stacks rather than compressing three columns on a phone", () => {
  assert.match(css, /@media \(max-width: 60rem\)/);
});
