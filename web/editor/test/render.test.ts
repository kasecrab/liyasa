// The shell's renderers.
//
// They are the one part of `src/editor.ts` that has decisions in it — what a
// problem row shows, whether a fix button appears, which label the main button
// carries — and they are pure, so they are testable without a browser. The
// mounting around them is not, and `web/e2e/a11y/editor.spec.ts` covers that
// against a real page.

import test from "node:test";
import assert from "node:assert/strict";

import type { Diagnostic, SourceDocument } from "../../../crates/liyasa-wasm/ts/liyasa-wasm.d.ts";
import { buildModel } from "../src/model.ts";
import { frontmatterDiagnostics, validateFrontmatter } from "../src/frontmatter.ts";
import {
  renderBlock,
  renderProblems,
  renderShortcuts,
  renderSurface,
  renderToolbar,
} from "../src/editor.ts";

function model(text: string) {
  const document: SourceDocument = {
    segments: [{ segment: "markdown", span: { source: 0, start: 0, end: new TextEncoder().encode(text).length } }],
    source: 0,
  };
  return buildModel(document, text);
}

test("a block carries its id, its kind and a role a screen reader can use", () => {
  const blocks = model("# Title\n\nbody\n").nodes[0]?.blocks ?? [];
  const markup = String(renderBlock(blocks[0]!));
  assert.match(markup, /data-block="0\.0"/);
  assert.match(markup, /class="block block-heading"/);
  assert.match(markup, /role="group"/);
  assert.match(markup, /tabindex="0"/);
});

test("a block's content is escaped, so a page cannot inject markup into the editor", () => {
  // The text is the author's and comes from a file somebody else may have
  // written. `<script>` in a paragraph is a paragraph about a script tag.
  const markup = String(renderBlock({ id: "0.0", kind: "paragraph", text: "<script>alert(1)</script>", start: 0, end: 25 }));
  assert.ok(!markup.includes("<script>"));
  assert.match(markup, /&lt;script&gt;/);
});

test("the surface renders one element per block and per unmodelled node", () => {
  const surface = String(renderSurface(model("first\n\nsecond\n\nthird\n")));
  assert.equal((surface.match(/data-block=/g) ?? []).length, 3);
  assert.match(surface, /data-editor-surface/);
});

test("a problem shows the plain-language message, and the detail beneath it", () => {
  const diagnostics: Diagnostic[] = [
    {
      code: "E0305",
      severity: "error",
      message: "Image without alt text",
      span: { source: 0, start: 6, end: 10 },
      url: "https://kasecrab.github.io/liyasa/docs/errors/E0305",
    },
  ];
  const markup = String(renderProblems("first\nsecond\n", diagnostics));
  assert.match(markup, /This image is missing a description for screen readers/);
  // The diagnostic's message here *is* the registry title, so it adds nothing
  // and is not repeated underneath.
  assert.ok(!markup.includes('class="detail"'), "a message that repeats the title is not shown twice");
  assert.match(markup, /Line 2/);
  assert.match(markup, /data-fix="add-alt-text"/);
  assert.match(markup, /class="problem problem-error"/);
});

test("a problem with no fix shows no button rather than a dead one", () => {
  const markup = String(
    renderProblems("body\n", [
      { code: "E0121", severity: "error", message: "m", url: "u" },
    ]),
  );
  assert.ok(!markup.includes("<button"), "E0121 has nothing the editor can do about it");
});

test("no problems is a sentence, not an empty list", () => {
  assert.equal(String(renderProblems("body\n", [])), '<p class="empty">No problems found.</p>');
});

test("a diagnostic from elsewhere cannot inject markup or a javascript: link", () => {
  // A `Diagnostic`'s `url` is a string in the payload. This editor renders
  // diagnostics that arrived over the network, and an author has every reason
  // to click the code beside a problem.
  const markup = String(
    renderProblems("body\n", [
      { code: "E9999", severity: "error", message: "<img onerror=alert(1)>", url: "javascript:alert(1)" },
    ]),
  );
  assert.ok(!markup.includes("<img"), "the message is escaped");
  assert.ok(!markup.includes("href="), "an off-origin url is not a link at all");
  assert.match(markup, /<span class="code">E9999<\/span>/);
});

test("a help link on the documentation origin is still a link", () => {
  const markup = String(
    renderProblems("body\n", [
      {
        code: "E0305",
        severity: "error",
        message: "Image without alt text",
        url: "https://kasecrab.github.io/liyasa/docs/errors/E0305",
      },
    ]),
  );
  assert.match(markup, /href="https:\/\/kasecrab\.github\.io\/liyasa\/docs\/errors\/E0305"/);
});

test("the toolbar's main button is one this person can press", () => {
  assert.match(String(renderToolbar({ role: "contributor" }, "visual", false)), /Submit for review/);
  assert.match(String(renderToolbar({ role: "editor" }, "visual", false)), /class="primary">Publish</);
});

test("the mode button names the mode it switches to", () => {
  assert.match(String(renderToolbar({ role: "editor" }, "visual", false)), /data-mode-switch[^>]*>\s*Source/);
  assert.match(String(renderToolbar({ role: "editor" }, "source", false)), /data-mode-switch[^>]*>\s*Visual/);
});

test("the git term appears only under the advanced disclosure", () => {
  assert.ok(!String(renderToolbar({ role: "editor" }, "visual", false)).includes("branch"));
  assert.match(String(renderToolbar({ role: "editor" }, "visual", true)), /draft \(branch\)/);
  assert.match(String(renderToolbar({ role: "editor" }, "visual", true)), /checked/);
});

test("the shortcut sheet is a table with a caption and header cells", () => {
  const markup = String(renderShortcuts());
  assert.match(markup, /<caption>Keyboard shortcuts<\/caption>/);
  assert.match(markup, /<th scope="col">Keys<\/th>/);
  assert.match(markup, /<kbd>Mod\+S<\/kbd>/);
  assert.ok((markup.match(/<tr>/g) ?? []).length > 15);
});

test("front matter findings become diagnostics the problems pane can show", () => {
  // The bridge between the form's validation and the pane. Without it the form
  // reports errors in one shape and the pane reads another.
  const schema = { properties: { title: { type: ["string", "null"] } } };
  const found = validateFrontmatter(schema, { title: 5 });
  const diagnostics = frontmatterDiagnostics(found);
  assert.equal(diagnostics.length, 1);
  assert.equal(diagnostics[0]?.code, "E0102");
  assert.equal(diagnostics[0]?.severity, "error");
  assert.match(diagnostics[0]?.url ?? "", /docs\/errors\/E0102$/);
  // ED-11: the pane says which field, not only that something is wrong.
  const markup = String(renderProblems("body\n", diagnostics));
  assert.match(markup, /A setting has the wrong kind of value/, "the plain-language headline");
  assert.match(markup, /`title` expects string, found number/, "and the detail that names the field");
});

test("a clean page produces no diagnostics", () => {
  const schema = { properties: { title: { type: ["string", "null"] } } };
  assert.deepEqual(frontmatterDiagnostics(validateFrontmatter(schema, { title: "A" })), []);
});
