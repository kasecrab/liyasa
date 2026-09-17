// ED-01's mapping and ED-03's round-trip contract, over hand-written
// `SourceDocument`s in the shape `liyasa_markdown::source::scan` produces.
//
// The fixtures here are segmentations, not editor nodes: `scan` is the subject
// on the other side of the seam and this package does not own it, so a fixture
// that invented its own node tree would be asserting against a model the
// product does not use. `tests/editor/ed_01_model.rs` runs the same mapping
// rules against the real scanner.

import test from "node:test";
import assert from "node:assert/strict";

import type { SourceDocument } from "../../../crates/liyasa-wasm/ts/liyasa-wasm.d.ts";
import {
  blockAt,
  buildModel,
  editBlock,
  flatten,
  serializeModel,
} from "../src/model.ts";

/** A segmentation of `text` written the way the scanner writes one. */
function scanned(text: string, segments: SourceDocument["segments"]): [string, SourceDocument] {
  return [text, { segments, source: 0 }];
}

function span(start: number, end: number) {
  return { source: 0, start, end };
}

test("prose, a chip, a logic block, a fence and a directive each become their node", () => {
  const text = "intro\n\n{{ name }}\n\n{% for r in rows %}\n- {{ r }}\n{% endfor %}\n";
  const forStart = text.indexOf("{% for");
  const endforStart = text.indexOf("{% endfor %}");
  const [source, document] = scanned(text, [
    { segment: "markdown", span: span(0, text.indexOf("{{ name }}")) },
    { segment: "template", kind: { kind: "output" }, span: span(7, 17) },
    { segment: "markdown", span: span(17, forStart) },
    {
      segment: "template",
      kind: { kind: "statement", name: "for", matching: 5 },
      span: span(forStart, forStart + "{% for r in rows %}".length),
    },
    { segment: "markdown", span: span(forStart + 19, endforStart) },
    { segment: "template", kind: { kind: "statement", name: "endfor" }, span: span(endforStart, text.length - 1) },
    { segment: "markdown", span: span(text.length - 1, text.length) },
  ]);

  const model = buildModel(document, source);
  const kinds = model.nodes.map((node) => node.kind);
  assert.deepEqual(kinds, ["markdown", "chip", "markdown", "logic_block", "markdown"]);

  const logic = model.nodes[3];
  assert.equal(logic?.name, "for");
  assert.equal(logic?.expression, "for r in rows");
  // ED-05: the body is a source mini-editor, so it is one opaque child holding
  // its bytes rather than a tree.
  assert.equal(logic?.children?.length, 1);
  assert.equal(logic?.children?.[0]?.kind, "opaque");
});

test("a directive becomes a component whose props are the scanner's", () => {
  const text = ':::note{type="warning"}\nbody\n:::\n';
  const [source, document] = scanned(text, [
    {
      segment: "directiveOpen",
      colons: 3,
      matching: 2,
      name: "note",
      props: { type: { type: "str", value: "warning" } },
      span: span(0, 24),
    },
    { segment: "markdown", span: span(24, 29) },
    { segment: "directiveClose", span: span(29, text.length) },
  ]);

  const model = buildModel(document, source);
  assert.equal(model.nodes.length, 1);
  const component = model.nodes[0];
  assert.equal(component?.kind, "component");
  assert.equal(component?.name, "note");
  assert.deepEqual(component?.props, { type: { type: "str", value: "warning" } });
  assert.equal(component?.children?.length, 1);
  assert.equal(component?.children?.[0]?.kind, "markdown");
});

test("an unclosed container carries its bytes rather than swallowing the rest", () => {
  const text = ":::note\nbody\n";
  const [source, document] = scanned(text, [
    { segment: "directiveOpen", colons: 3, matching: null, name: "note", props: {}, span: span(0, 8) },
    { segment: "markdown", span: span(8, text.length) },
  ]);

  const model = buildModel(document, source);
  assert.equal(model.nodes[0]?.kind, "opaque");
  assert.equal(serializeModel(model), text);
});

test("a raw HTML block inside a markdown segment is an opaque block", () => {
  const text = "before\n\n<div class=x>\nraw\n</div>\n\nafter\n";
  const [source, document] = scanned(text, [{ segment: "markdown", span: span(0, text.length) }]);

  const model = buildModel(document, source);
  const blocks = model.nodes[0]?.blocks ?? [];
  assert.deepEqual(
    blocks.map((block) => block.kind),
    ["paragraph", "html", "paragraph"],
  );
  assert.equal(blocks[1]?.text, "<div class=x>\nraw\n</div>\n\n");
});

test("a heading, a list, a quote and a table are classified", () => {
  const text = "# Title\n\n- one\n- two\n\n> quoted\n\n| a | b |\n| - | - |\n| 1 | 2 |\n";
  const [source, document] = scanned(text, [{ segment: "markdown", span: span(0, text.length) }]);
  const blocks = buildModel(document, source).nodes[0]?.blocks ?? [];
  assert.deepEqual(
    blocks.map((block) => block.kind),
    ["heading", "list", "quote", "table"],
  );
  assert.equal(blocks[0]?.level, 1);
});

test("ED-03(a): concatenating the model reproduces the source byte for byte", () => {
  const cases = [
    "",
    "# Title\n\nbody\n",
    "```bash\necho {{ x }}\n```\n",
    "trailing text with no newline",
    "unicode ☃ and an emoji 🎈 in prose\n",
  ];
  for (const text of cases) {
    const [source, document] = scanned(text, [{ segment: "markdown", span: span(0, byteLength(text)) }]);
    assert.equal(serializeModel(buildModel(document, source)), text);
  }
});

test("ED-03(a): spans are byte offsets, not UTF-16 offsets", () => {
  // "é" is two bytes and one UTF-16 unit; a model that sliced by index would
  // cut the paragraph one character short and still look right in ASCII.
  const text = "café\n\nbar\n";
  const split = byteLength("café\n\n");
  const [source, document] = scanned(text, [
    { segment: "markdown", span: span(0, split) },
    { segment: "markdown", span: span(split, byteLength(text)) },
  ]);
  const model = buildModel(document, source);
  assert.equal(model.nodes[0]?.text, "café\n\n");
  assert.equal(model.nodes[1]?.text, "bar\n");
  assert.equal(serializeModel(model), text);
});

test("ED-03(b): editing one block names one segment and changes only its bytes", () => {
  const text = "first para\n\nsecond para\n\nthird para\n";
  const [source, document] = scanned(text, [{ segment: "markdown", span: span(0, byteLength(text)) }]);
  const model = buildModel(document, source);

  const second = blockAt(model, "0.1");
  assert.equal(second?.text, "second para\n\n");

  const edits = editBlock(model, "0.1", "SECOND para\n\n");
  assert.deepEqual(edits, [{ segment: 0, new_text: "first para\n\nSECOND para\n\nthird para\n" }]);
  // Every byte outside the edited block survived.
  assert.ok(edits[0]?.new_text.startsWith("first para\n\n"));
  assert.ok(edits[0]?.new_text.endsWith("third para\n"));
});

test("ED-03(b): editing a block in one segment leaves the other segments unedited", () => {
  const text = "a\n\n{{ x }}\n\nb\n";
  const [source, document] = scanned(text, [
    { segment: "markdown", span: span(0, 3) },
    { segment: "template", kind: { kind: "output" }, span: span(3, 10) },
    { segment: "markdown", span: span(10, byteLength(text)) },
  ]);
  const model = buildModel(document, source);
  assert.deepEqual(editBlock(model, "2.0", "B\n"), [{ segment: 2, new_text: "\n\nB\n" }]);
  assert.deepEqual(editBlock(model, "1", "{{ y }}"), [{ segment: 1, new_text: "{{ y }}" }]);
});

test("ED-03(b): an edit that does not change the text produces no edit at all", () => {
  const text = "only\n";
  const [source, document] = scanned(text, [{ segment: "markdown", span: span(0, 5) }]);
  const model = buildModel(document, source);
  assert.deepEqual(editBlock(model, "0.0", "only\n"), []);
});

test("every node in the model is reachable by its id", () => {
  const text = ":::note\nbody\n\nmore\n:::\n";
  const [source, document] = scanned(text, [
    { segment: "directiveOpen", colons: 3, matching: 2, name: "note", props: {}, span: span(0, 8) },
    { segment: "markdown", span: span(8, 19) },
    { segment: "directiveClose", span: span(19, byteLength(text)) },
  ]);
  const model = buildModel(document, source);
  for (const node of flatten(model)) {
    assert.equal(blockAt(model, node.id)?.id, node.id, `${node.id} is reachable`);
  }
});

function byteLength(text: string): number {
  return new TextEncoder().encode(text).length;
}
