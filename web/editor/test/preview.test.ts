// ED-05: what a chip shows, what a logic block shows beside it, and the two
// caps the editor's own preview obeys.

import test from "node:test";
import assert from "node:assert/strict";

import type { ExpansionRecord } from "../../../crates/liyasa-wasm/ts/liyasa-wasm.d.ts";
import {
  PREVIEW_DEFAULTS,
  capBytes,
  capIterations,
  chipSource,
  previewContext,
  previewLimits,
} from "../src/preview.ts";

test("the defaults are ED-05's, not the build's", () => {
  assert.deepEqual(PREVIEW_DEFAULTS, { maxIterations: 50, maxBytes: 256 * 1024 });
  assert.deepEqual(previewLimits({}).limits, PREVIEW_DEFAULTS);
});

test("a configured limit is read from liyasa.json", () => {
  const read = previewLimits({ editor: { preview: { maxIterations: 5, maxBytes: "1MB" } } });
  assert.deepEqual(read.limits, { maxIterations: 5, maxBytes: 1024 * 1024 });
  assert.deepEqual(read.diagnostics, []);
});

test("every byte size the schema's pattern allows is understood", () => {
  for (const [written, expected] of [
    ["512B", 512],
    ["256KB", 256 * 1024],
    ["1.5MB", Math.round(1.5 * 1024 * 1024)],
    ["2 GB", 2 * 1024 * 1024 * 1024],
  ] as const) {
    assert.equal(previewLimits({ editor: { preview: { maxBytes: written } } }).limits.maxBytes, expected, written);
  }
});

test("a size the schema would reject is a diagnostic, not a silent default", () => {
  // Falling back quietly means the author configured a cap, the editor used a
  // different one, and nothing said so.
  const read = previewLimits({ editor: { preview: { maxBytes: "loads" } } });
  assert.equal(read.limits.maxBytes, PREVIEW_DEFAULTS.maxBytes);
  assert.equal(read.diagnostics[0]?.code, "E0102");
  assert.match(read.diagnostics[0]?.message ?? "", /editor\.preview\.maxBytes/);
});

test("a non-positive iteration count is a diagnostic too", () => {
  const read = previewLimits({ editor: { preview: { maxIterations: 0 } } });
  assert.equal(read.limits.maxIterations, PREVIEW_DEFAULTS.maxIterations);
  assert.equal(read.diagnostics[0]?.code, "E0102");
});

test("a capped loop says how many rows there are, not only how many it drew", () => {
  // "50 of 50" and "50 of 900" are different statements and the author needs
  // the second one to know the preview is partial.
  const rows = Array.from({ length: 900 }, (_, at) => `row ${at}`);
  const capped = capIterations(rows, { maxIterations: 50, maxBytes: 1024 });
  assert.equal(capped.shown.length, 50);
  assert.equal(capped.total, 900);
  assert.equal(capped.truncated, true);
});

test("a loop under the cap is not reported as truncated", () => {
  const capped = capIterations(["a", "b"], PREVIEW_DEFAULTS);
  assert.deepEqual(capped, { shown: ["a", "b"], total: 2, truncated: false });
});

test("the byte cap cuts on a character boundary", () => {
  // Cutting a UTF-8 sequence in half puts a replacement character in the
  // author's preview and makes the editor look like it corrupted the page.
  const text = "☃".repeat(10); // three bytes each
  const capped = capBytes(text, { maxIterations: 50, maxBytes: 8 });
  assert.equal(capped.truncated, true);
  assert.equal(capped.text, "☃☃");
  assert.ok(!capped.text.includes("�"));
});

test("text inside the byte cap is returned unchanged", () => {
  assert.deepEqual(capBytes("short", PREVIEW_DEFAULTS), { text: "short", truncated: false, bytes: 5 });
});

test("the toolbar's selection becomes the context the build would use", () => {
  const context = previewContext(
    { version: "2.0", locale: "fr", readerGroups: ["beta", "staff"] },
    { site: { name: "Acme" }, facts: { plan: { price: 99 } } },
  );
  // A dimension sits at the root, the way `Layers::values` writes it.
  assert.equal(context["version"], "2.0");
  assert.equal(context["locale"], "fr");
  assert.deepEqual(context["reader"], { groups: ["beta", "staff"] });
  assert.deepEqual(context["facts"], { plan: { price: 99 } });
});

test("a dimension the toolbar did not select is absent, not empty", () => {
  // An empty string is a version named "", and `by_version[""]` is a lookup
  // the build would not have made.
  const context = previewContext({ readerGroups: [] }, {});
  assert.ok(!("version" in context));
  assert.ok(!("locale" in context));
  assert.ok(!("reader" in context), "no groups means no reader layer at all");
});

test("a chip names where its value came from", () => {
  const record: ExpansionRecord = {
    dimensions: ["version"],
    env: ["API_URL"],
    facts: ["plan.pro.price"],
    includes: [],
    reader_fields: ["groups"],
  };
  assert.deepEqual(chipSource('fact("plan.pro.price")', record), {
    kind: "fact",
    name: "plan.pro.price",
    detail: "from facts/",
  });
  assert.deepEqual(chipSource('env("API_URL")', record), {
    kind: "env",
    name: "API_URL",
    detail: "from build.env",
  });
  assert.deepEqual(chipSource("reader.groups", record), {
    kind: "reader",
    name: "reader.groups",
    detail: "from the reader, per request",
  });
  assert.deepEqual(chipSource("version", record), {
    kind: "dimension",
    name: "version",
    detail: "from the preview context",
  });
});

test("a chip whose expression the record does not mention says so", () => {
  // Claiming a source the record does not carry is the editor asserting
  // something it did not check.
  const record: ExpansionRecord = { dimensions: [], env: [], facts: [], includes: [], reader_fields: [] };
  assert.deepEqual(chipSource("page.title", record), {
    kind: "expression",
    name: "page.title",
    detail: "an expression over the preview context",
  });
});
