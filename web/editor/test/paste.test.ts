// ED-04's paste: HTML from another application in, Liyasa Markdown out.
//
// The fixtures are real clipboard HTML shapes, not markup written to suit the
// converter: Google Docs wraps everything in a `<b>` whose `font-weight` is
// `normal` and marks bold with an inline style, Notion emits `<figure>` and
// `<div class="indented">`, Confluence emits `<ac:*>` elements and a
// `confluenceTable`. A converter tested only against clean HTML passes and
// then mangles every real paste.

import { readFileSync, readdirSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";
import assert from "node:assert/strict";

import { htmlToMarkdown, imagesIn, pasteSource } from "../src/paste.ts";

const HERE = dirname(fileURLToPath(import.meta.url));
const FIXTURES = resolve(HERE, "fixtures/paste");

function fixture(name: string): string {
  return readFileSync(resolve(FIXTURES, name), "utf8");
}

test("every fixture has a golden file and converts to it", () => {
  const inputs = readdirSync(FIXTURES).filter((name) => name.endsWith(".html"));
  assert.ok(inputs.length >= 5, "the named sources and the fragment case are covered");
  for (const name of inputs) {
    const golden = fixture(name.replace(/\.html$/, ".md"));
    assert.equal(htmlToMarkdown(fixture(name)), golden, `${name} converts to its golden file`);
  }
});

test("the source application is recognised from the markup", () => {
  assert.equal(pasteSource(fixture("google-docs.html")), "google-docs");
  assert.equal(pasteSource(fixture("notion.html")), "notion");
  assert.equal(pasteSource(fixture("confluence.html")), "confluence");
  assert.equal(pasteSource(fixture("html.html")), "html");
});

test("a Google Docs bold wrapper is not read as bold", () => {
  // The whole paste is inside `<b style="font-weight:normal">`. Treating it as
  // bold marks the entire paste `**...**`, which is what a converter that
  // reads tag names does.
  //
  // The fragment fixture is the one that reaches the rule, and this test was
  // asserted against the document fixture first, where it could not: there the
  // wrapper holds `h1`, `p` and `ul`, so it is a container and the inline path
  // never runs. Removing the guard left the assertion green. A partial
  // selection is the shape that reaches it, and it is the common paste.
  const fragment = htmlToMarkdown(fixture("google-docs-fragment.html"));
  assert.equal(fragment, "Set the flag and then **restart** the server.\n");

  const document = htmlToMarkdown(fixture("google-docs.html"));
  assert.ok(!document.startsWith("**"), "the wrapper is not bold");
  assert.ok(document.includes("**really bold**"), "real bold survives");
});

test("a bold tag with no style override is still bold at block position", () => {
  // The other side of the same rule: `inline()` on a container drops the
  // container's own emphasis, so `<b>` alone rendered as plain text.
  assert.equal(htmlToMarkdown("<b>shouted</b>"), "**shouted**\n");
  assert.equal(htmlToMarkdown(`<b style="font-weight:normal">quiet</b>`), "quiet\n");
});

test("images are collected for the media library rather than inlined as data", () => {
  const images = imagesIn(fixture("notion.html"));
  assert.deepEqual(
    images.map((image) => image.alt),
    ["A architecture diagram"],
  );
  assert.equal(images[0]?.src, "https://example.invalid/diagram.png");
  // The Markdown names the asset the library will hold, not the remote URL:
  // a pasted image that stays remote is a link that breaks when the source
  // does, and CM-131 never sees the bytes.
  assert.ok(htmlToMarkdown(fixture("notion.html")).includes("![A architecture diagram](/assets/diagram.png)"));
});

test("script and style never survive a paste", () => {
  const converted = htmlToMarkdown(
    `<div><script>alert(1)</script><style>p{color:red}</style><p>text</p></div>`,
  );
  assert.equal(converted, "text\n");
});

test("markdown control characters in pasted text are escaped", () => {
  assert.equal(htmlToMarkdown("<p>a * b _ c [d] # e</p>"), "a \\* b \\_ c \\[d\\] # e\n");
  // A heading marker at the start of a line would change the block's meaning.
  assert.equal(htmlToMarkdown("<p># not a heading</p>"), "\\# not a heading\n");
});

test("an empty paste is an empty string, not a stray newline", () => {
  assert.equal(htmlToMarkdown(""), "");
  assert.equal(htmlToMarkdown("<div>   </div>"), "");
});
