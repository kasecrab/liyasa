// ED-73: every code an author can meet has plain-language text, and the table
// names no code the registry does not have.

import test from "node:test";
import assert from "node:assert/strict";

import { CODES, EDITOR_CRATES } from "../src/code-list.ts";
import { MESSAGES, editorCodes, messageFor, uncovered } from "../src/messages.ts";

test("the generated code list is the registry's, not a copy somebody typed", () => {
  // If this file were hand-written it would drift the first time a package
  // added a code. `tests/editor/ed_73_messages.rs` rewrites it and fails.
  assert.ok(CODES.length > 150, "the whole registry is here");
  assert.ok(CODES.every((entry) => /^[EW]\d{4}$/.test(entry.code)));
  assert.deepEqual(EDITOR_CRATES, [
    "liyasa-config",
    "liyasa-markdown",
    "liyasa-components",
    "liyasa-search",
    "liyasa-wasm",
  ]);
});

test("ED-73: every code the editor's validator can raise has plain-language text", () => {
  assert.deepEqual(uncovered(), [], "these codes have no plain-language message");
});

test("the table names no code the registry does not have", () => {
  const known = new Set(CODES.map((entry) => entry.code));
  for (const code of Object.keys(MESSAGES)) {
    assert.ok(known.has(code), `${code} is not in the registry`);
  }
});

test("the table covers only editor-reachable codes", () => {
  // A message for a `liyasa-build` code would be this package rewording
  // another package's diagnostic, which the author meets through a build with
  // the build's own words.
  const reachable = new Set(editorCodes().map((entry) => entry.code));
  for (const code of Object.keys(MESSAGES)) {
    assert.ok(reachable.has(code), `${code} is not a code the editor raises`);
  }
});

test("a plain message says something the title does not", () => {
  // A "plain-language variant" that repeats the title is the requirement
  // satisfied on paper and not at all in practice.
  for (const entry of editorCodes()) {
    const message = MESSAGES[entry.code];
    assert.ok(message, entry.code);
    assert.notEqual(message.plain, entry.title, `${entry.code} just repeats its title`);
    assert.ok(message.plain.length > entry.title.length / 2, `${entry.code}'s message is too short to say anything`);
  }
});

test("a plain message is a sentence, not a fragment", () => {
  for (const entry of editorCodes()) {
    const plain = MESSAGES[entry.code]?.plain ?? "";
    assert.match(plain, /^[A-Z“`]/, `${entry.code} does not start like a sentence`);
    assert.match(plain, /[.!?]$/, `${entry.code} does not end like a sentence`);
  }
});

test("ED-73: the image case reads the way the requirement's example does", () => {
  const shown = messageFor("E0305");
  assert.equal(
    shown.plain,
    "This image is missing a description for screen readers. Say what the image shows, not that it is a screenshot.",
  );
  assert.equal(shown.fix?.action, "add-alt-text");
});

test("a fix names an action the editor can actually perform", () => {
  const performable = new Set([
    "open-config",
    "open-frontmatter",
    "add-alt-text",
    "close-container",
    "remove-directive-close",
    "add-required-prop",
    "remove-unknown-prop",
    "rename-to-suggestion",
    "fix-heading-level",
    "escape-character",
    "open-navigation",
    "declare-personalized",
    "preview-on-server",
  ]);
  for (const [code, message] of Object.entries(MESSAGES)) {
    if (!message.fix) continue;
    assert.ok(performable.has(message.fix.action), `${code} names an action nothing performs`);
    assert.ok(message.fix.label.length > 2, `${code}'s button has no label`);
  }
});

test("not every code has a fix, and that is the honest answer", () => {
  // A fix button on everything would mean a button that opens a dialog and
  // says "now do it yourself".
  const withFix = Object.values(MESSAGES).filter((message) => message.fix).length;
  assert.ok(withFix > 10, "the codes that can be fixed have a fix");
  assert.ok(withFix < Object.keys(MESSAGES).length, "not everything claims a fix");
});

test("a code with no plain text falls back to the title and says so", () => {
  // A build-side code reaching the editor's pane is possible; inventing a
  // friendly sentence for it would be the editor making something up.
  const shown = messageFor("E0701");
  assert.equal(shown.hasPlain, false);
  assert.equal(shown.plain, shown.title);
});

test("an unregistered code does not crash the pane", () => {
  const shown = messageFor("E9999");
  assert.equal(shown.title, "E9999");
  assert.equal(shown.hasPlain, false);
});
