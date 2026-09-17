// ED-73: every code an author can meet has plain-language text, and the table
// names no code the registry does not have.
//
// The registry is read here, from `crates/liyasa-core/src/diagnostics/codes.toml`,
// rather than from a generated copy of it. RFC 2433 records why: a checked-in
// artefact derived from a *shared append-only* file goes stale whenever any
// other package merges a code, which took this branch red in `integrate` on
// eight rows nobody here wrote.
//
// Parsing TOML with a regular expression is usually a bad idea and is fine
// here for one reason: `codes.toml` is one line per row by construction — its
// own header says so, because `merge=union` requires it — and
// `cargo test -p liyasa-core registry` already rejects a row that is not. The
// count assertion below is what makes a format change fail loudly instead of
// matching nothing and passing.

import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";
import assert from "node:assert/strict";

import { MESSAGES, messageFor, shownFor } from "../src/messages.ts";

const HERE = dirname(fileURLToPath(import.meta.url));
const REGISTRY = resolve(HERE, "../../../crates/liyasa-core/src/diagnostics/codes.toml");

interface CodeEntry {
  code: string;
  severity: string;
  crate: string;
  title: string;
}

/**
 * The crates whose diagnostics the WebAssembly module can raise in a browser.
 *
 * WP-24a's notes list what `liyasa-wasm` carries: the scanner, expansion, the
 * directive parser, the components, the sanitizer, config and front matter
 * validation and the `liyasa-idx` reader. A code from `liyasa-build` or
 * `liyasa-server` reaches an author through a *build* — a preview, a
 * verification result — and arrives as a `Diagnostic` carrying its own
 * message, which the pane shows. It is not this package's to reword.
 *
 * `tests/editor/ed_73_messages.rs` asserts that each of these really raises
 * codes, which is how `liyasa-core` was caught in an earlier version of this
 * list: it owns the registry and raises nothing.
 */
const EDITOR_CRATES = [
  "liyasa-config",
  "liyasa-markdown",
  "liyasa-components",
  "liyasa-search",
  "liyasa-wasm",
];

const ROW = /^([EW]\d{4})\s*=\s*\{([^}]*)\}\s*$/;
const FIELD = (name: string) => new RegExp(`${name}\\s*=\\s*"((?:[^"\\\\]|\\\\.)*)"`);

function registry(): CodeEntry[] {
  const source = readFileSync(REGISTRY, "utf8");
  const found: CodeEntry[] = [];
  for (const line of source.split("\n")) {
    const row = ROW.exec(line);
    if (!row) continue;
    const body = row[2] as string;
    found.push({
      code: row[1] as string,
      severity: FIELD("severity").exec(body)?.[1] ?? "",
      crate: FIELD("crate").exec(body)?.[1] ?? "",
      title: (FIELD("title").exec(body)?.[1] ?? "").replace(/\\"/g, '"'),
    });
  }
  return found;
}

const CODES = registry();

function editorCodes(): CodeEntry[] {
  return CODES.filter((entry) => EDITOR_CRATES.includes(entry.crate));
}

test("the registry parses, and the parse is not silently empty", () => {
  // The whole point of the count: a `codes.toml` that changed shape would
  // otherwise match nothing here and every coverage assertion below would
  // pass on an empty set.
  assert.ok(CODES.length > 150, `only ${CODES.length} rows parsed out of codes.toml`);
  assert.ok(editorCodes().length > 50, "the editor-reachable subset is not empty");
  for (const entry of CODES) {
    assert.match(entry.code, /^[EW]\d{4}$/);
    assert.match(entry.severity, /^(error|warning|info|hint)$/, `${entry.code} has a severity`);
    assert.match(entry.crate, /^liyasa-[a-z]+$/, `${entry.code} names its crate`);
    assert.ok(entry.title.length > 5, `${entry.code} has a title`);
  }
});

test("the prefix and the severity agree, which is what makes the parse trustworthy", () => {
  // `liyasa-core`'s own registry test enforces this. Asserting it here too is
  // how this parse proves it read the fields it thinks it read, rather than
  // matching the right shape against the wrong columns.
  for (const entry of CODES) {
    const expected = entry.code.startsWith("E") ? "error" : "warning";
    assert.equal(entry.severity, expected, `${entry.code}`);
  }
});

test("ED-73: every code the editor's validator can raise has plain-language text", () => {
  const uncovered = editorCodes()
    .map((entry) => entry.code)
    .filter((code) => MESSAGES[code] === undefined);
  assert.deepEqual(uncovered, [], "these codes have no plain-language message");
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
    shown?.plain,
    "This image is missing a description for screen readers. Say what the image shows, not that it is a screenshot.",
  );
  assert.equal(shown?.fix?.action, "add-alt-text");
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

test("a code with no plain text falls back to the diagnostic's own message", () => {
  // RFC 2433: this is what let the generated registry copy go. `E0701` is a
  // `liyasa-build` code; its own message is more specific than the registry
  // title the editor used to look up.
  const shown = shownFor({ code: "E0701", message: "the build wrote two files to `out/index.html`" });
  assert.equal(shown.hasPlain, false);
  assert.equal(shown.headline, "the build wrote two files to `out/index.html`");
  assert.equal(shown.detail, null, "the message is the headline, not repeated beneath it");
  assert.equal(shown.fix, null);
});

test("a code with plain text shows it, and the detail beneath when it adds something", () => {
  const shown = shownFor({ code: "E0102", message: "`title` expects string, found number" });
  assert.equal(shown.hasPlain, true);
  assert.match(shown.headline, /A setting has the wrong kind of value/);
  assert.equal(shown.detail, "`title` expects string, found number");
  assert.equal(shown.fix?.action, "open-config");
});

test("a message that repeats the headline is not shown twice", () => {
  const plain = messageFor("E0305")?.plain ?? "";
  assert.equal(shownFor({ code: "E0305", message: plain }).detail, null);
});

test("a diagnostic with no message at all falls back to its code", () => {
  // Rather than an empty headline, which is a problem row that says nothing.
  assert.equal(shownFor({ code: "E9999", message: "   " }).headline, "E9999");
});

test("every one of WP-28's new server codes is handled without a message of ours", () => {
  // The eight rows that took this branch red in `integrate`. They are
  // `liyasa-server`'s, so this package writes no text for them — and now
  // costs nothing when they land.
  for (const code of ["E0850", "E0851", "E0852", "E0853", "E0854", "E0855", "E0856", "E0857"]) {
    const entry = CODES.find((candidate) => candidate.code === code);
    if (!entry) continue; // not yet on this branch's main; nothing to assert
    assert.equal(entry.crate, "liyasa-server");
    assert.equal(MESSAGES[code], undefined, `${code} is not this package's to reword`);
    assert.equal(shownFor({ code, message: "something went wrong" }).headline, "something went wrong");
  }
});
