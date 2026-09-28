// ED-05's chip and logic block, and the rest of the visual surface.
//
// The corpus is the real scanner's output, from `fixtures/segments.json`, so the
// nodes these renderers are given are the nodes the product builds.

import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";
import assert from "node:assert/strict";

import type { ExpansionRecord, SourceDocument } from "../../../crates/liyasa-wasm/ts/liyasa-wasm.d.ts";
import { buildModel, flatten } from "../src/model.ts";
import type { EditorNode } from "../src/model.ts";
import { capIterations, chipSource, PREVIEW_DEFAULTS } from "../src/preview.ts";
import {
  chipTooltip,
  expressionCompletions,
  renderChip,
  renderCode,
  renderComponent,
  renderExpressionEditor,
  renderLogicBlock,
  renderNode,
  renderOpaque,
  renderSurface,
  unexpanded,
} from "../src/view/blocks.ts";
import type { SurfaceContext } from "../src/view/blocks.ts";

const HERE = dirname(fileURLToPath(import.meta.url));
const CORPUS: { path: string; source: string; document: SourceDocument }[] = JSON.parse(
  readFileSync(resolve(HERE, "fixtures/segments.json"), "utf8"),
);

function corpus(path: string) {
  const page = CORPUS.find((candidate) => candidate.path === path);
  assert.ok(page, `${path} is in the corpus`);
  return buildModel(page.document, page.source);
}

function nodeOf(path: string, kind: EditorNode["kind"]): EditorNode {
  // `flatten` returns blocks as well as nodes; the two kind sets do not overlap,
  // so matching on the kind is enough to pick a node out.
  const found = flatten(corpus(path)).find((each) => "kind" in each && each.kind === kind);
  assert.ok(found, `${path} has a ${kind} node`);
  return found as EditorNode;
}

const RECORD: ExpansionRecord = {
  dimensions: ["version"],
  env: ["CI"],
  facts: ["price", "release.version"],
  includes: [],
  reader_fields: ["groups"],
};

// --- ED-05: the chip --------------------------------------------------------

test("a chip with no resolved value says so instead of showing the expression as one", () => {
  // The editor can always print the expression and can never print the value
  // without having been told. A chip that shows `fact("price")` styled as a
  // value is a page the author proof-reads against fiction.
  const node: EditorNode = { id: "0", kind: "chip", segment: 0, text: '{{ fact("price") }}', expression: ' fact("price") ' };
  const markup = String(renderChip(node, { record: RECORD, values: {} }));
  assert.match(markup, /data-unresolved/);
  assert.ok(!markup.includes("chip-value"), "nothing is presented as a value");
  assert.match(markup, /No value for the selected context yet\./);
});

test("a chip with a resolved value shows the value and not the expression", () => {
  const node: EditorNode = { id: "0", kind: "chip", segment: 0, text: '{{ fact("price") }}', expression: ' fact("price") ' };
  const markup = String(renderChip(node, { record: RECORD, values: { 'fact("price")': "£9" } }));
  assert.match(markup, /<span class="chip-value">£9<\/span>/);
  assert.ok(!markup.includes("data-unresolved"));
  assert.ok(!markup.includes("No value"), "and it does not hedge about a value it has");
});

test("the tooltip names the source the record recorded, and nothing else", () => {
  // `chipSource` reads the page's own expansion record, so the tooltip cannot
  // claim a fact the expansion never resolved.
  assert.match(chipTooltip(chipSource('fact("price")', RECORD), "£9"), /^price, from facts\/\.$/);
  assert.match(chipTooltip(chipSource('fact("nope")', RECORD), "£9"), /an expression over the preview context/);
  assert.match(chipTooltip(chipSource("version", RECORD), "2.0"), /from the preview context/);
});

test("the tooltip is reachable by a screen reader, not only on hover", () => {
  // A `title` attribute is not announced by every screen reader and never on a
  // touch device, so the source is a described-by element instead.
  const node: EditorNode = { id: "1/0", kind: "chip", segment: 3, text: "{{ version }}", expression: " version " };
  const markup = String(renderChip(node, { record: RECORD, values: { version: "2.0" } }));
  assert.match(markup, /aria-describedby="chip-1-0-source"/);
  assert.match(markup, /id="chip-1-0-source"/);
  assert.match(markup, /tabindex="0"/, "and it is focusable");
});

test("an expression is escaped, so a page cannot inject markup through a chip", () => {
  const node: EditorNode = { id: "0", kind: "chip", segment: 0, text: "x", expression: '<img src=x onerror="alert(1)">' };
  const markup = String(renderChip(node, unexpanded()));
  assert.ok(!markup.includes("<img"), markup);
  assert.match(markup, /&lt;img/);
});

// --- ED-05: the logic block -------------------------------------------------

test("the logic block gives the statement, the body and the expansion their own controls", () => {
  const node = nodeOf("limits.md", "logic_block");
  const markup = String(renderLogicBlock(node, unexpanded()));
  assert.match(markup, /data-expression="/, "the statement is editable");
  assert.match(markup, /data-block-body="/, "the body is editable");
  assert.match(markup, /data-expansion="/, "and the result has somewhere to go");
});

test("the body is a source mini-editor, and says why", () => {
  // ED-05 names this and gives the reason: a loop body is frequently partial
  // Markdown, and no visual surface can represent half a table.
  const markup = String(renderLogicBlock(nodeOf("limits.md", "logic_block"), unexpanded()));
  assert.match(markup, /<textarea class="source-input"[^>]*data-block-body=/);
  assert.match(markup, /partial Markdown/);
  assert.match(markup, /cannot show half a table/);
});

test("an unexpanded loop says there is no result rather than showing an empty one", () => {
  const markup = String(renderLogicBlock(nodeOf("limits.md", "logic_block"), unexpanded()));
  assert.match(markup, /Nothing has expanded this page for the selected context yet/);
  assert.ok(!markup.includes("<pre class=\"expansion\""), "no empty output box");
  assert.ok(!markup.includes("data-expanded-count"), "and no count of nothing");
});

test("an expanded loop shows the rows read-only, capped, with the whole total", () => {
  const rows = Array.from({ length: 900 }, (_, at) => `| row ${at} |\n`);
  const node = nodeOf("limits.md", "logic_block");
  const context: SurfaceContext = {
    ...unexpanded(),
    expansions: { [node.id]: capIterations(rows, PREVIEW_DEFAULTS) },
  };
  const markup = String(renderLogicBlock(node, context));
  // The name says read-only; `aria-readonly` on a `<pre>` is not an allowed
  // attribute and every screen reader ignores it. Focusable because the
  // stylesheet scrolls the box, and an unreachable scroll region is unreadable
  // content (WCAG 2.1.1); axe's `scrollable-region-focusable` caught that one.
  assert.match(markup, /<pre class="expansion" data-readonly tabindex="0" aria-label="[^"]*Read-only/);
  assert.ok(!markup.includes("aria-readonly"), "not on a pre");
  assert.match(markup, /Showing 50 of 900 rows/);
  assert.ok(markup.includes("| row 0 |"), "the first row is drawn");
  assert.ok(!markup.includes("| row 50 |"), "and the fifty-first is not");
});

test("the statement editor offers the scanner's own completions", () => {
  // The same source the source pane uses, so the two cannot drift into offering
  // different words for the same position.
  const node: EditorNode = { id: "0", kind: "logic_block", segment: 0, text: "{% fo %}", name: "for", expression: " fo" };
  const offered = expressionCompletions(node, unexpanded()).map((option) => option.label);
  assert.ok(offered.includes("for"), offered.join(","));
  const markup = String(renderExpressionEditor(node, unexpanded()));
  assert.match(markup, /list="logic-0-completions"/);
  assert.match(markup, /<datalist id="logic-0-completions">/);
  assert.match(markup, /<option value="for">/);
});

test("the statement input is labelled, and an `if` is labelled as a condition", () => {
  const loop: EditorNode = { id: "0", kind: "logic_block", segment: 0, text: "x", name: "for", expression: "for row in rows" };
  const when: EditorNode = { id: "1", kind: "logic_block", segment: 1, text: "x", name: "if", expression: "if reader.staff" };
  assert.match(String(renderExpressionEditor(loop, unexpanded())), /<label for="logic-0-expression">Statement<\/label>/);
  assert.match(String(renderExpressionEditor(when, unexpanded())), /<label for="logic-1-expression">Condition<\/label>/);
  assert.match(String(renderLogicBlock(when, unexpanded())), /Show only when/);
  assert.match(String(renderLogicBlock(loop, unexpanded())), /Repeat for each/);
});

// --- the rest of the surface ------------------------------------------------

test("a component offers its properties form rather than embedding one", () => {
  const node = nodeOf("limits.md", "component");
  const markup = String(renderComponent(node, unexpanded()));
  assert.match(markup, new RegExp(`data-open-properties="${node.id.replace(/\//g, "\\/")}"`));
  assert.match(markup, new RegExp(`data-component="${node.name}"`));
  assert.ok(!markup.includes("data-prop="), "the fields belong to the properties pane");
});

test("a code node keeps its language and edits its body, not its fence", () => {
  const node: EditorNode = { id: "0", kind: "code", segment: 0, text: "```rust\nfn main() {}\n```\n", lang: "rust" };
  const markup = String(renderCode(node));
  assert.match(markup, /data-lang="rust"/);
  assert.match(markup, /Code \(rust\)/);
  assert.match(markup, /<textarea[^>]*>fn main\(\) \{\}<\/textarea>/);
  assert.ok(!markup.includes("```"), "the fence is the editor's business, not the author's");
});

test("a fence with no language says Code rather than inventing one", () => {
  const node: EditorNode = { id: "0", kind: "code", segment: 0, text: "```\nx\n```\n", lang: null };
  const markup = String(renderCode(node));
  assert.match(markup, /data-lang=""/);
  assert.match(markup, />Code<\/label>/);
});

test("an opaque node shows its bytes, says why, and offers no form", () => {
  const node: EditorNode = { id: "0", kind: "opaque", segment: 0, text: '<div class="legacy">x</div>\n' };
  const markup = String(renderOpaque(node));
  assert.match(markup, /raw HTML/);
  assert.match(markup, /&lt;div class=&quot;legacy&quot;&gt;/);
  assert.match(markup, /data-edit-source="0"/);
  assert.ok(!markup.includes("data-prop="), "no form for something the editor does not model");
});

test("every node kind ED-01 names renders as itself, not as a fallback", () => {
  // The failure this stops: a kind with no case in `renderNode`, which falls to
  // the opaque branch and looks like a working editor showing a text dump. The
  // marker per kind is what distinguishes "rendered" from "fell through".
  const MARKER: Record<string, RegExp> = {
    markdown: /class="block block-paragraph"/,
    chip: /data-chip=/,
    logic_block: /data-expansion=/,
    component: /data-open-properties=/,
    code: /data-code=/,
    opaque: /data-opaque/,
  };
  for (const [kind, marker] of Object.entries(MARKER)) {
    const node: EditorNode = {
      id: "0",
      kind: kind as EditorNode["kind"],
      segment: 0,
      text: "x\n",
      blocks: kind === "markdown" ? [{ id: "0.0", kind: "paragraph", text: "x\n", start: 0, end: 2 }] : undefined,
    };
    assert.match(String(renderNode(node, unexpanded())), marker, `${kind} renders as ${kind}`);
  }
  assert.equal(Object.keys(MARKER).length, 6, "the six kinds ED-01 names");
});

test("the corpus exercises the kinds these renderers claim to cover", () => {
  // Without this the table above is a list of kinds I invented; with it, at
  // least four of the six are shapes the real scanner produces.
  const kinds = new Set<string>();
  for (const page of CORPUS) {
    for (const node of buildModel(page.document, page.source).nodes) kinds.add(node.kind);
  }
  for (const kind of ["markdown", "chip", "logic_block", "component"]) {
    assert.ok(kinds.has(kind), `the corpus has a ${kind} node; it has ${[...kinds].join(", ")}`);
  }
});

test("the surface renders every node of every corpus page without throwing", () => {
  for (const page of CORPUS) {
    const markup = String(renderSurface(buildModel(page.document, page.source), unexpanded()));
    assert.match(markup, /data-editor-surface/, page.path);
    assert.ok(!markup.includes("undefined"), `${page.path} has no undefined in it`);
  }
});
