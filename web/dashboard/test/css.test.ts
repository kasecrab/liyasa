// ANA-70: the dashboard "follow[s] the design system in light and dark".
//
// The check is mechanical and the interesting part is what it excludes: the
// fallback `:root` blocks at the top of the file are where the tokens get
// values when the theme stylesheet is not loaded. Everything after them must
// name a token, or the dashboard has a second palette and dark mode is this
// package's problem rather than the theme's.

import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";
import assert from "node:assert/strict";

const HERE = dirname(fileURLToPath(import.meta.url));
const css = readFileSync(resolve(HERE, "..", "dashboard.css"), "utf8");

/** The file with the three fallback blocks cut out. */
function withoutFallbacks(): string {
  const start = css.indexOf("body {");
  assert.ok(start > 0, "the fallback blocks come first and `body` follows them");
  return css.slice(start);
}

test("no rule outside the fallback blocks names a literal colour", () => {
  const body = withoutFallbacks();
  const literals = body.match(/#[0-9a-fA-F]{3,8}\b|rgba?\(|hsla?\(/g) ?? [];
  assert.deepEqual(literals, [], "a second palette is a second dark mode to maintain");
});

test("the tokens the dashboard uses are the theme's names", () => {
  const used = new Set(Array.from(css.matchAll(/var\((--ly-[a-z0-9-]+)\)/g), (m) => m[1]));
  assert.ok(used.size > 20, "the dashboard is built out of the design system");
  for (const token of used) {
    assert.match(
      token as string,
      /^--ly-(color|space|radius|text|font|shadow|motion|z)-|^--ly-border(-|$)/,
      token as string,
    );
  }
});

test("every token the file uses is one the file also defines a fallback for", () => {
  // Otherwise the dashboard renders with an empty value when opened without
  // the theme stylesheet: no colour at all rather than a plain one.
  const defined = new Set(Array.from(css.matchAll(/^\s{2}(--ly-[a-z0-9-]+):/gm), (m) => m[1]));
  const used = new Set(Array.from(css.matchAll(/var\((--ly-[a-z0-9-]+)\)/g), (m) => m[1]));
  const missing = Array.from(used).filter((token) => !defined.has(token as string));
  assert.deepEqual(missing, []);
});

test("dark mode is offered by preference and by an explicit attribute", () => {
  assert.match(css, /@media \(prefers-color-scheme: dark\)/);
  assert.match(css, /:root:not\(\[data-theme="light"\]\)/, "an explicit light choice wins");
  assert.match(css, /:root\[data-theme="dark"\]/, "and an explicit dark choice works too");
});

test("focus is visible", () => {
  assert.match(css, /:focus-visible\s*\{[^}]*outline:/);
});
