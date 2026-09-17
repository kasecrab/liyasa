// The escaper, which every interpolation in this package goes through.

import test from "node:test";
import assert from "node:assert/strict";

import { Fragment, escapeHtml, html, raw } from "../src/escape.ts";

test("a value is escaped and a fragment is not", () => {
  const inner = html`<em>${"<b>"}</em>`;
  assert.equal(String(inner), "<em>&lt;b&gt;</em>");
  assert.equal(String(html`<p>${inner}</p>`), "<p><em>&lt;b&gt;</em></p>");
});

test("the five characters that end an attribute or a tag are escaped", () => {
  assert.equal(escapeHtml(`&<>"'`), "&amp;&lt;&gt;&quot;&#39;");
});

test("nothing renders for null and undefined, and an array joins", () => {
  assert.equal(String(html`${null}${undefined}`), "");
  assert.equal(String(html`${["a", "<b>"]}`), "a&lt;b&gt;");
});

test("raw marks a string as markup", () => {
  assert.ok(raw("<hr>") instanceof Fragment);
  assert.equal(String(html`${raw("<hr>")}`), "<hr>");
});
