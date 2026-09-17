// ED-02: the source mode's highlighting, its autocomplete, and where a
// diagnostic from the WebAssembly validator lands.

import test from "node:test";
import assert from "node:assert/strict";

import type { Diagnostic, SourceDocument } from "../../../crates/liyasa-wasm/ts/liyasa-wasm.d.ts";
import { completionsAt, decorate, placeDiagnostics, tokenize } from "../src/source.ts";

const bytes = (text: string) => new TextEncoder().encode(text).length;

function scanned(text: string, segments: SourceDocument["segments"]): SourceDocument {
  return { segments, source: 0 };
}

test("highlighting comes from the segmentation, not from a second parser", () => {
  // A source mode with its own tokenizer disagrees with the visual mode about
  // what a construct is, and the author sees one of the two answers depending
  // on which pane they are in.
  const text = "para\n{{ x }}\n:::note\nbody\n:::\n";
  const document = scanned(text, [
    { segment: "markdown", span: { source: 0, start: 0, end: 5 } },
    { segment: "template", kind: { kind: "output" }, span: { source: 0, start: 5, end: 12 } },
    { segment: "markdown", span: { source: 0, start: 12, end: 13 } },
    { segment: "directiveOpen", colons: 3, matching: 5, name: "note", props: {}, span: { source: 0, start: 13, end: 21 } },
    { segment: "markdown", span: { source: 0, start: 21, end: 26 } },
    { segment: "directiveClose", span: { source: 0, start: 26, end: bytes(text) } },
  ]);

  const tokens = tokenize(document, text);
  assert.deepEqual(
    tokens.map((token) => token.kind),
    ["markdown", "template-output", "markdown", "directive", "markdown", "directive"],
  );
  // Concatenating the tokens reproduces the file: a highlighter that drops a
  // byte shows an editor missing text the file has.
  assert.equal(tokens.map((token) => text.slice(token.start, token.end)).join(""), text);
});

test("token offsets index the string the view holds, not the bytes the API sent", () => {
  // The API's spans are byte offsets and a textarea is indexed by UTF-16 unit.
  // With any non-ASCII character above it, a token highlighted at the byte
  // offset covers the wrong text — and every test written in ASCII passes.
  const text = "café\n{{ x }}\n";
  const split = bytes("café\n");
  const document = scanned(text, [
    { segment: "markdown", span: { source: 0, start: 0, end: split } },
    { segment: "template", kind: { kind: "output" }, span: { source: 0, start: split, end: split + 7 } },
    { segment: "markdown", span: { source: 0, start: split + 7, end: bytes(text) } },
  ]);
  const tokens = tokenize(document, text);
  assert.equal(text.slice(tokens[1]?.start, tokens[1]?.end), "{{ x }}");
  assert.equal(tokens.map((token) => text.slice(token.start, token.end)).join(""), text);
});

test("front matter is its own token and is not markdown", () => {
  const text = "---\ntitle: A\n---\nbody\n";
  const document: SourceDocument = {
    frontmatter: { span: { source: 0, start: 0, end: 17 }, fields: {} as never },
    segments: [{ segment: "markdown", span: { source: 0, start: 17, end: bytes(text) } }],
    source: 0,
  };
  assert.equal(tokenize(document, text)[0]?.kind, "frontmatter");
});

test("a markdown token is decorated down to headings, emphasis, links and code", () => {
  const text = "# Title\n\nSee `x` and [a](/b), **bold** and *thin*.\n";
  const spans = decorate(text, 0);
  const kinds = spans.map((span) => span.kind);
  for (const wanted of ["heading", "inline-code", "link", "strong", "emphasis"]) {
    assert.ok(kinds.includes(wanted as (typeof kinds)[number]), `${wanted} is decorated`);
  }
  // `**bold**` is strong and `*thin*` is emphasis; a pattern that reads the
  // first as emphasis underlines one asterisk of it and nothing else.
  const emphasis = spans.find((span) => span.kind === "emphasis");
  assert.equal(text.slice(emphasis?.start, emphasis?.end), "*thin*");
  // Offsets are absolute, so the view does not have to know where the token
  // started.
  const link = spans.find((span) => span.kind === "link");
  assert.equal(text.slice(link?.start, link?.end), "[a](/b)");
});

test("decoration offsets are relative to the file, not to the token", () => {
  const spans = decorate("`x`", 40);
  assert.deepEqual(spans, [{ kind: "inline-code", start: 40, end: 43 }]);
});

test("an offset inside an output expression completes functions, not components", () => {
  const text = "body\n{{ fa";
  const names = completionsAt(text, text.length, { components: ["note", "tabs"], facts: ["plan.pro.price"] })
    .map((completion) => completion.label);
  assert.ok(names.includes("fact("), "the function the fact slash command writes");
  assert.ok(!names.includes("note"), "a component is not a template function");
});

test("an offset after a directive marker completes declared component names", () => {
  const text = "body\n:::no";
  const names = completionsAt(text, text.length, { components: ["note", "tabs"] }).map((item) => item.label);
  assert.deepEqual(names, ["note"]);
});

test("an offset inside a directive's props completes that component's props", () => {
  const text = ':::note{ti';
  const names = completionsAt(text, text.length, {
    components: ["note"],
    props: { note: ["title", "icon"] },
  }).map((item) => item.label);
  assert.deepEqual(names, ["title"]);
});

test("an offset inside a statement completes statements", () => {
  const names = completionsAt("{% f", 4, {}).map((item) => item.label);
  assert.ok(names.includes("for"));
  assert.ok(!names.includes("if"), "`if` does not contain `f` at the start");
});

test("a fact name completes inside fact()", () => {
  const text = '{{ fact("plan.';
  const names = completionsAt(text, text.length, { facts: ["plan.pro.price", "team.size"] })
    .map((item) => item.label);
  assert.deepEqual(names, ["plan.pro.price"]);
});

test("plain prose offers nothing", () => {
  assert.deepEqual(completionsAt("just writing a sentence", 12, { components: ["note"] }), []);
});

test("a diagnostic lands on the line and column its byte span names", () => {
  const text = "first\nsecond {{ x }}\nthird\n";
  const diagnostic: Diagnostic = {
    code: "E0201",
    message: "Undefined template variable",
    severity: "error",
    span: { source: 0, start: 13, end: 20 },
    url: "https://kasecrab.github.io/liyasa/docs/errors/E0201",
  };
  const placed = placeDiagnostics(text, [diagnostic]);
  assert.deepEqual(placed[0]?.from, { line: 2, column: 8 });
  assert.deepEqual(placed[0]?.to, { line: 2, column: 15 });
  assert.equal(placed[0]?.diagnostic.code, "E0201");
});

test("a diagnostic with no span is placed at the top rather than dropped", () => {
  // A validator finding about the file as a whole has no span. Dropping it is
  // how a source mode reports success on a page that does not build.
  const placed = placeDiagnostics("body\n", [
    { code: "E0102", message: "Config does not match schema", severity: "error", url: "u" },
  ]);
  assert.equal(placed.length, 1);
  assert.deepEqual(placed[0]?.from, { line: 1, column: 1 });
});

test("columns count characters, not bytes", () => {
  // "é" is two bytes. A column taken from the byte offset puts the underline
  // one character to the right of the problem for every line above it that
  // holds one.
  const text = "café {{ x }}\n";
  const placed = placeDiagnostics(text, [
    { code: "E0201", message: "m", severity: "error", span: { source: 0, start: 6, end: 13 }, url: "u" },
  ]);
  assert.deepEqual(placed[0]?.from, { line: 1, column: 6 });
});
