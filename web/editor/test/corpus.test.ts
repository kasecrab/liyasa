// The model, asserted against segmentations the real scanner produced.
//
// `test/model.test.ts` writes its own `SourceDocument`s, which is right for
// pinning one rule at a time but proves nothing about the shapes
// `liyasa_markdown::source::scan` actually emits. `fixtures/segments.json` is
// written by `tests/editor/segments.rs` from the pages under `fixtures/pages/`
// and fails that Rust test when it drifts, so what is asserted here is the
// product's own output.

import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";
import assert from "node:assert/strict";

import type { SourceDocument } from "../../../crates/liyasa-wasm/ts/liyasa-wasm.d.ts";
import { buildModel, editBlock, flatten, serializeModel } from "../src/model.ts";
import { tokenize } from "../src/source.ts";

const HERE = dirname(fileURLToPath(import.meta.url));

interface Scanned {
  path: string;
  source: string;
  document: SourceDocument;
}

const CORPUS: Scanned[] = JSON.parse(readFileSync(resolve(HERE, "fixtures/segments.json"), "utf8"));

test("the corpus holds the pages the fixture claims", () => {
  assert.deepEqual(
    CORPUS.map((page) => page.path),
    ["install.md", "limits.md", "plain.md"],
  );
});

test("ED-03(a): every corpus page round trips through the model byte for byte", () => {
  for (const { path, source, document } of CORPUS) {
    assert.equal(serializeModel(buildModel(document, source)), source, `${path} round trips`);
  }
});

test("ED-01: a real page maps to the node kinds the requirement names", () => {
  const limits = CORPUS.find((page) => page.path === "limits.md");
  assert.ok(limits, "the fixture has limits.md");
  const model = buildModel(limits.document, limits.source);
  const kinds = new Set(model.nodes.map((node) => node.kind));
  for (const wanted of ["markdown", "chip", "logic_block", "component", "code"]) {
    assert.ok(kinds.has(wanted as never), `${wanted} appears in limits.md`);
  }
});

test("ED-01: a directive becomes a component carrying the scanner's props", () => {
  const limits = CORPUS.find((page) => page.path === "limits.md");
  const note = buildModel(limits!.document, limits!.source).nodes.find(
    (node) => node.kind === "component" && node.name === "note",
  );
  assert.ok(note, "the `:::note` is a component node");
  assert.deepEqual(note.props, { title: { type: "str", value: "Heads up" } });
  assert.ok((note.children ?? []).length > 0, "its body is its children");
});

test("ED-01: a leaf directive is a component with no children", () => {
  const limits = CORPUS.find((page) => page.path === "limits.md");
  const image = buildModel(limits!.document, limits!.source).nodes.find(
    (node) => node.kind === "component" && node.name === "image",
  );
  assert.ok(image, "the `::image` is a component node");
  assert.deepEqual(image.children, []);
  assert.equal(image.props?.["alt"]?.type, "str");
});

test("ED-03(c): the raw HTML block is an opaque block carrying its bytes", () => {
  const limits = CORPUS.find((page) => page.path === "limits.md");
  const model = buildModel(limits!.document, limits!.source);
  const html = model.nodes
    .flatMap((node) => node.blocks ?? [])
    .find((block) => block.kind === "html");
  assert.ok(html, "the `<div class=\"legacy\">` is an html block");
  assert.ok(html.text.includes("Raw HTML the visual editor does not model."));
});

test("ED-03(b): editing any block of any corpus page changes only that block's bytes", () => {
  for (const { path, source, document } of CORPUS) {
    const model = buildModel(document, source);
    for (const item of flatten(model)) {
      if (!("start" in item)) continue; // only markdown blocks are edited as text
      const edits = editBlock(model, item.id, `EDITED\n\n`);
      assert.equal(edits.length, 1, `${path} ${item.id} edits one segment`);
      const edit = edits[0]!;
      const segment = document.segments[edit.segment]!;
      const before = source.slice(0, byteToIndex(source, segment.span.start));
      const after = source.slice(byteToIndex(source, segment.span.end));
      const written = before + edit.new_text + after;
      assert.ok(written.startsWith(before), `${path} ${item.id} left the bytes before it alone`);
      assert.ok(written.endsWith(after), `${path} ${item.id} left the bytes after it alone`);
      assert.ok(written.includes("EDITED"), `${path} ${item.id} actually changed`);
    }
  }
});

test("a nested container's children are the nodes between its fences", () => {
  // `install.md` is `::::tabs` holding two `:::tab`s, each holding a fence.
  const install = CORPUS.find((page) => page.path === "install.md");
  const model = buildModel(install!.document, install!.source);
  const tabs = model.nodes.find((node) => node.kind === "component" && node.name === "tabs");
  assert.ok(tabs, "the outer container is one node");
  const inner = (tabs.children ?? []).filter((child) => child.kind === "component");
  assert.deepEqual(inner.map((child) => child.name), ["tab", "tab"]);
  assert.deepEqual(
    (inner[0]?.children ?? []).map((child) => child.kind),
    ["code"],
  );
});

test("the source mode's tokens cover every corpus page with no gaps", () => {
  for (const { path, source, document } of CORPUS) {
    const tokens = tokenize(document, source);
    assert.equal(
      tokens.map((token) => source.slice(token.start, token.end)).join(""),
      source,
      `${path} is tokenized with no gaps and no overlap`,
    );
  }
});

test("a page with front matter has it as a token and outside every node", () => {
  const limits = CORPUS.find((page) => page.path === "limits.md");
  assert.ok(limits!.document.frontmatter, "limits.md has front matter");
  assert.equal(tokenize(limits!.document, limits!.source)[0]?.kind, "frontmatter");
  const model = buildModel(limits!.document, limits!.source);
  assert.ok(model.frontmatter.startsWith("---\n"));
  assert.ok(!model.nodes.some((node) => node.text.includes("title: Limits")));
});

function byteToIndex(text: string, offset: number): number {
  const encoder = new TextEncoder();
  let bytes = 0;
  let index = 0;
  for (const character of text) {
    if (bytes >= offset) break;
    bytes += encoder.encode(character).length;
    index += character.length;
  }
  return index;
}
