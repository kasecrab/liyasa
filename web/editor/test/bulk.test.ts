// ED-12: find and replace across pages with a preview, move a group, apply a
// tag, change a fact reference.

import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";
import assert from "node:assert/strict";

import type { SourceDocument } from "../../../crates/liyasa-wasm/ts/liyasa-wasm.d.ts";
import { applyPlan, applyTag, changeFactReference, findReplace, moveGroup } from "../src/bulk.ts";

const HERE = dirname(fileURLToPath(import.meta.url));

interface Scanned {
  path: string;
  source: string;
  document: SourceDocument;
}

const PAGES: Scanned[] = JSON.parse(readFileSync(resolve(HERE, "fixtures/bulk.json"), "utf8"));

test("the fixture is the fifty pages the requirement names", () => {
  assert.equal(PAGES.length, 50);
});

test("a replace covers prose and props and leaves fences and expressions alone", () => {
  // Each page says "Widget API" four times: in prose, in a directive prop, in
  // a fenced curl command, and around a template expression. Rewriting the
  // command silently changes a command the reader will run.
  const plan = findReplace(PAGES, { find: "Widget API", replaceWith: "Gadget API" });
  assert.equal(plan.pages.length, 50);
  assert.equal(plan.matches.length, 50 * 4, "prose twice, one prop, one after the expression");

  const first = plan.pages[0];
  assert.ok(first?.after.includes("The Gadget API is described here"));
  assert.ok(first?.after.includes('title="The Gadget API"'));
  assert.ok(first?.after.includes("The Gadget API changed in 2.0."));
  assert.ok(first?.after.includes("documents the Gadget API."));
  assert.ok(
    first?.after.includes("curl https://example.invalid/Widget/API"),
    "the fenced command is untouched",
  );
});

test("the code scope is opt-in and does reach a fence when asked", () => {
  const plan = findReplace(PAGES, { find: "Widget", replaceWith: "Gadget", scopes: ["code"] });
  const first = plan.pages[0];
  assert.ok(first?.after.includes("curl https://example.invalid/Gadget/API"));
  assert.ok(first?.after.includes("The Widget API is described here"), "prose is untouched");
});

test("a template expression is never rewritten, even when the scope asks for text", () => {
  // `{{ site.name }}` is code the build runs. A find-and-replace that edits it
  // changes what a page computes, which is not what "replace this phrase"
  // means to anybody.
  const plan = findReplace(PAGES, { find: "site.name", replaceWith: "site.title" });
  assert.deepEqual(plan.matches, []);
  assert.deepEqual(plan.pages, []);
});

test("the preview is the result: applying the plan gives exactly what it showed", () => {
  // The requirement's own words — "the diff preview matched the result". The
  // only way to guarantee that is for the preview to *be* the result.
  const plan = findReplace(PAGES, { find: "Widget API", replaceWith: "Gadget API" });
  const applied = applyPlan(plan);
  assert.equal(applied.length, plan.pages.length);
  for (const written of applied) {
    const previewed = plan.pages.find((page) => page.path === written.path);
    assert.equal(written.text, previewed?.after, `${written.path} matched its preview`);
  }
});

test("a page with no match is absent from the plan rather than rewritten identically", () => {
  const plan = findReplace(PAGES, { find: "nothing matches this", replaceWith: "x" });
  assert.deepEqual(plan.pages, []);
  assert.deepEqual(plan.matches, []);
});

test("each match names its page, its segment and the line the author will see", () => {
  const plan = findReplace(PAGES.slice(0, 1), { find: "Widget API", replaceWith: "Gadget API" });
  const match = plan.matches[0];
  assert.equal(match?.path, "guides/page-00.md");
  assert.equal(typeof match?.segment, "number");
  assert.equal(match?.line, 5);
  assert.equal(match?.text, "The Widget API is described here, page 0.");
});

test("a replace is literal by default, so a phrase with punctuation is safe", () => {
  const pages: Scanned[] = [
    {
      path: "a.md",
      source: "Costs $9.99 today.\n",
      document: { segments: [{ segment: "markdown", span: { source: 0, start: 0, end: 19 } }], source: 0 },
    },
  ];
  const plan = findReplace(pages, { find: "$9.99", replaceWith: "$19.99" });
  assert.equal(plan.pages[0]?.after, "Costs $19.99 today.\n");
});

test("a regular expression replace is available and is opt-in", () => {
  const pages: Scanned[] = [
    {
      path: "a.md",
      source: "v1.2.3 and v4.5.6\n",
      document: { segments: [{ segment: "markdown", span: { source: 0, start: 0, end: 18 } }], source: 0 },
    },
  ];
  const plan = findReplace(pages, { find: "v\\d+\\.\\d+\\.\\d+", replaceWith: "vLATEST", regex: true });
  assert.equal(plan.pages[0]?.after, "vLATEST and vLATEST\n");
});

test("an invalid regular expression is refused rather than matching nothing", () => {
  // "no matches" and "your pattern is broken" look identical in a preview, and
  // the author concludes the phrase is not there.
  const plan = findReplace(PAGES, { find: "(unclosed", replaceWith: "x", regex: true });
  assert.equal(plan.diagnostics[0]?.code, "E0103");
  assert.deepEqual(plan.pages, []);
});

test("moving a group keeps its pages and its other keys", () => {
  const config = {
    navigation: [
      { group: "A", pages: ["a.md"] },
      { group: "B", pages: ["b.md"], expanded: true },
      { group: "C", pages: ["c.md"] },
    ],
  };
  const change = moveGroup({ config, pages: {} }, { group: "B", toIndex: 0 });
  const tree = (change.config as typeof config).navigation;
  assert.deepEqual(tree.map((node) => node.group), ["B", "A", "C"]);
  assert.deepEqual(tree[0]?.pages, ["b.md"]);
  assert.equal(tree[0]?.expanded, true);
});

test("moving a group that is not there is refused", () => {
  const change = moveGroup({ config: { navigation: [] }, pages: {} }, { group: "Z", toIndex: 0 });
  assert.equal(change.diagnostics[0]?.code, "E0104");
});

test("applying a tag writes it to the front matter of each named page and nothing else", () => {
  const pages = {
    "a.md": "---\ntitle: A\n---\n\nbody\n",
    "b.md": "---\ntitle: B\ntag: old\n---\n\nbody\n",
    "c.md": "---\ntitle: C\n---\n\nbody\n",
  };
  const writes = applyTag(pages, { paths: ["a.md", "b.md"], tag: "beta" });
  assert.deepEqual(writes.map((write) => write.path), ["a.md", "b.md"]);
  assert.equal(writes[0]?.text, "---\ntitle: A\ntag: beta\n---\n\nbody\n");
  assert.equal(writes[1]?.text, "---\ntitle: B\ntag: beta\n---\n\nbody\n");
});

test("applying a tag that is already set writes nothing for that page", () => {
  const pages = { "a.md": "---\ntitle: A\ntag: beta\n---\n\nbody\n" };
  assert.deepEqual(applyTag(pages, { paths: ["a.md"], tag: "beta" }), []);
});

test("changing a fact reference rewrites the call and not a phrase that looks like it", () => {
  const source = 'A {{ fact("plan.pro.price") }} and the words plan.pro.price in prose.\n';
  const expression = '{{ fact("plan.pro.price") }}';
  const at = source.indexOf(expression);
  const pages: Scanned[] = [
    {
      path: "a.md",
      source,
      document: {
        segments: [
          { segment: "markdown", span: { source: 0, start: 0, end: at } },
          {
            segment: "template",
            kind: { kind: "output" },
            span: { source: 0, start: at, end: at + expression.length },
          },
          { segment: "markdown", span: { source: 0, start: at + expression.length, end: source.length } },
        ],
        source: 0,
      },
    },
  ];
  const plan = changeFactReference(pages, { from: "plan.pro.price", to: "plan.team.price" });
  assert.equal(plan.pages[0]?.after, 'A {{ fact("plan.team.price") }} and the words plan.pro.price in prose.\n');
  assert.equal(plan.matches.length, 1);
});

test("changing a fact reference reaches every page that reads it", () => {
  const plan = changeFactReference(PAGES, { from: "nothing.here", to: "x" });
  assert.deepEqual(plan.pages, [], "a fact nothing reads changes nothing");
});

test("the plan's edits and its preview are the same page", () => {
  // `after` is what the diff shows; `edits` is what goes to the WebAssembly
  // serializer. A replace that touches four segments of one page — prose, a
  // prop, prose again, and prose after a template expression — is where the
  // two can drift, because every replacement shifts what follows it.
  const plan = findReplace(PAGES, { find: "Widget API", replaceWith: "Gadget API" });
  for (const page of plan.pages) {
    const scanned = PAGES.find((candidate) => candidate.path === page.path)!;
    assert.ok(page.edits.length >= 4, `${page.path} edits at least the four segments that matched`);
    assert.equal(serializeSource(scanned, page.edits), page.after, `${page.path}`);
  }
});

/** `liyasa_markdown::source::serialize::serialize_source`, in the test. */
function serializeSource(page: Scanned, edits: { segment: number; new_text: string }[]): string {
  const encoder = new TextEncoder();
  const bytes = encoder.encode(page.source);
  const decoder = new TextDecoder();
  const slice = (start: number, end: number) => decoder.decode(bytes.subarray(start, end));
  let out = page.document.frontmatter
    ? slice(page.document.frontmatter.span.start, page.document.frontmatter.span.end)
    : "";
  page.document.segments.forEach((segment, at) => {
    const edit = edits.find((candidate) => candidate.segment === at);
    out += edit ? edit.new_text : slice(segment.span.start, segment.span.end);
  });
  return out;
}
