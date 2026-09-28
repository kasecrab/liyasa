// ED-02's source pane and ED-03(c)'s source popover.
//
// The corpus is the real scanner's output, from `fixtures/segments.json`, so
// what the highlighter is asserted against is what the product segments.

import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";
import assert from "node:assert/strict";

import type { SourceDocument } from "../../../crates/liyasa-wasm/ts/liyasa-wasm.d.ts";
import { tokenize } from "../src/source.ts";
import { highlight, opaqueReason, renderSourceMode, renderSourcePopover } from "../src/view/source-mode.ts";

const HERE = dirname(fileURLToPath(import.meta.url));
const CORPUS: { path: string; source: string; document: SourceDocument }[] = JSON.parse(
  readFileSync(resolve(HERE, "fixtures/segments.json"), "utf8"),
);

/** The text a layer shows, with its markup stripped. */
function shown(markup: string): string {
  return markup
    .replace(/<[^>]*>/g, "")
    .replace(/&lt;/g, "<")
    .replace(/&gt;/g, ">")
    .replace(/&quot;/g, '"')
    .replace(/&#39;/g, "'")
    .replace(/&amp;/g, "&");
}

test("the highlighted layer reproduces every corpus page exactly", () => {
  // The layers are stacked, so a dropped or duplicated byte puts the caret in
  // the wrong place — the failure is not cosmetic.
  for (const page of CORPUS) {
    const markup = String(highlight(tokenize(page.document, page.source), page.source));
    assert.equal(shown(markup), page.source, `${page.path} is reproduced`);
  }
});

test("the layer marks the constructs the segmentation found", () => {
  const limits = CORPUS.find((page) => page.path === "limits.md");
  assert.ok(limits);
  const markup = String(highlight(tokenize(limits.document, limits.source), limits.source));
  for (const kind of ["directive", "template-output", "code"]) {
    assert.ok(markup.includes(`data-token="${kind}"`), `${kind} is marked`);
  }
});

test("a markdown run gets its inline marks without printing a byte twice", () => {
  const source = "# Title\n\nSee `x` and [a](/b), **bold** and *thin*.\n";
  const document: SourceDocument = {
    segments: [{ segment: "markdown", span: { source: 0, start: 0, end: new TextEncoder().encode(source).length } }],
    source: 0,
  };
  const markup = String(highlight(tokenize(document, source), source));
  assert.equal(shown(markup), source);
  assert.ok(markup.includes('data-token="heading"'));
  assert.ok(markup.includes('data-token="inline-code"'));
  assert.ok(markup.includes('data-token="link"'));
});

test("overlapping decorations do not duplicate text", () => {
  // `**bold**` inside a heading is both a heading and strong. Nesting them
  // naively prints the bytes twice and the layers stop lining up.
  const source = "## A **bold** heading\n";
  const document: SourceDocument = {
    segments: [{ segment: "markdown", span: { source: 0, start: 0, end: source.length } }],
    source: 0,
  };
  assert.equal(shown(String(highlight(tokenize(document, source), source))), source);
});

test("the pane has a gutter line per source line", () => {
  const page = CORPUS.find((candidate) => candidate.path === "plain.md")!;
  const markup = String(renderSourceMode({ document: page.document, source: page.source, problemLines: [] }));
  const lines = page.source.split("\n").length;
  assert.equal((markup.match(/data-gutter-line=/g) ?? []).length, lines);
});

test("a line with a problem is marked in the gutter by severity", () => {
  const page = CORPUS.find((candidate) => candidate.path === "plain.md")!;
  const markup = String(
    renderSourceMode({
      document: page.document,
      source: page.source,
      problemLines: [{ line: 2, severity: "error" }],
    }),
  );
  assert.match(markup, /class="gutter-line gutter-error" data-gutter-line="2"/);
});

test("the textarea owns the caret and the highlight is hidden from a screen reader", () => {
  // A screen reader reading coloured spans as well as the textarea reads the
  // page twice. The textarea is the accessible control; the layer is paint.
  const page = CORPUS.find((candidate) => candidate.path === "plain.md")!;
  const markup = String(renderSourceMode({ document: page.document, source: page.source, problemLines: [] }));
  assert.match(markup, /<pre class="highlight" aria-hidden="true"/);
  assert.match(markup, /<textarea[^>]*data-source-editor/);
  assert.match(markup, /aria-label="Page source"/);
  assert.match(markup, /<div class="gutter" aria-hidden="true">/);
});

test("the source in the textarea is escaped, so a page cannot close it early", () => {
  const source = "</textarea><script>alert(1)</script>\n";
  const document: SourceDocument = {
    segments: [{ segment: "markdown", span: { source: 0, start: 0, end: source.length } }],
    source: 0,
  };
  const markup = String(renderSourceMode({ document, source, problemLines: [] }));
  assert.ok(!markup.includes("<script>"));
  assert.ok(!markup.includes("</textarea><script"));
  assert.match(markup, /&lt;\/textarea&gt;/);
});

test("ED-03(c): the popover shows the bytes and offers no form", () => {
  // An opaque node is opaque because the editor has no model for it. A form
  // here would claim an understanding it does not have.
  const markup = String(
    renderSourcePopover({ block: "0.1", text: "<div>raw</div>\n", reason: "This is raw HTML." }),
  );
  assert.match(markup, /role="dialog"/);
  assert.match(markup, /data-source-popover="0\.1"/);
  assert.match(markup, /<textarea[^>]*data-source-editor/);
  assert.match(markup, /&lt;div&gt;raw&lt;\/div&gt;/);
  assert.ok(!markup.includes("data-prop="), "no properties form");
  assert.match(markup, /data-popover-apply="0\.1"/);
  assert.match(markup, /data-popover-cancel="0\.1"/);
});

test("the popover is labelled and sized to its content, within bounds", () => {
  const short = String(renderSourcePopover({ block: "a", text: "x\n", reason: "r" }));
  assert.match(short, /aria-labelledby="popover-a-title"/);
  assert.match(short, /rows="3"/, "a short block still gets a usable box");
  const long = String(renderSourcePopover({ block: "b", text: "x\n".repeat(80), reason: "r" }));
  assert.match(long, /rows="20"/, "a long block does not grow without limit");
});

test("the reason names the shape rather than giving one generic sentence", () => {
  // Three of these four are not mistakes, and telling an author "not modelled"
  // for all of them says nothing about whether they need to act.
  assert.match(opaqueReason("html", '<div class="legacy">x</div>'), /raw HTML/);
  assert.match(opaqueReason("opaque", "{# a note #}"), /template comment/);
  assert.match(opaqueReason("opaque", ":::note\nbody\n"), /not closed/);
  assert.match(opaqueReason("opaque", "{% for row in rows %}"), /template statement/);
});

test("an unclosed container's reason says what to do about it", () => {
  // The one of the four that *is* a mistake.
  assert.match(opaqueReason("opaque", ":::note\nbody\n"), /Editing it as source is how to fix that/);
});

test("a shape with no special reason still names its kind", () => {
  assert.match(opaqueReason("directive close", ":::\n"), /not closed|directive close/);
});

test("a gap before the first segment is still painted, not dropped", () => {
  // Reachable, and through a defect this package reported: when front matter
  // fails to parse, `scan_frontmatter` returns `None` while still advancing
  // past the block, so `document.frontmatter` is null and segment 0 starts
  // at byte 43. Without the gap guard the layer omits those 43 characters and
  // the caret sits 43 columns out for the whole file — a mis-parse in the front
  // matter would silently break typing in the body.
  const source = '---\nid: not-a-ulid\ntitle: Limits\n---\n\nbody\n';
  const body = source.indexOf("\nbody\n") + 1;
  const document: SourceDocument = {
    segments: [{ segment: "markdown", span: { source: 0, start: body, end: source.length } }],
    source: 0,
  };
  const markup = String(highlight(tokenize(document, source), source));
  assert.equal(shown(markup), source, "every byte is painted, including the unowned front matter");
});

test("a trailing gap after the last segment is painted too", () => {
  const source = "body\ntrailing\n";
  const document: SourceDocument = {
    segments: [{ segment: "markdown", span: { source: 0, start: 0, end: 5 } }],
    source: 0,
  };
  assert.equal(shown(String(highlight(tokenize(document, source), source))), source);
});
