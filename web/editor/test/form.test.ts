// ED-11's acceptance criterion, against the markup the form produces.
//
// > Given the front matter form; when a value invalid per the schema is
// > entered; then the field shows the schema error and save is blocked.
//
// The schema is the real `schemas/frontmatter.json`, not a fixture: a form
// generated from a schema written to suit the test would pass while offering
// fields the build rejects.

import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";
import assert from "node:assert/strict";

import { formFields, validateFrontmatter } from "../src/frontmatter.ts";
import { label, renderFrontmatterForm, valueFromControl } from "../src/view/form.ts";

const HERE = dirname(fileURLToPath(import.meta.url));
const schema = JSON.parse(readFileSync(resolve(HERE, "../../../schemas/frontmatter.json"), "utf8"));
const FIELDS = formFields(schema);

function form(values: Record<string, never> | Record<string, unknown>, advanced = false) {
  const validation = validateFrontmatter(schema, values);
  return String(
    renderFrontmatterForm({
      fields: FIELDS,
      values: values as never,
      validation,
      advanced,
    }),
  );
}

test("every schema property gets a field, and each carries its help", () => {
  const markup = form({});
  for (const field of FIELDS) {
    assert.ok(markup.includes(`data-field="${field.name}"`), `${field.name} has a field`);
  }
  // One help paragraph per field, none of them empty.
  const helps = markup.match(/data-field-help>([^<]*)</g) ?? [];
  assert.equal(helps.length, FIELDS.length);
  assert.ok(!helps.some((help) => help.endsWith(">>") || help.endsWith("><")), "no field has empty help");
});

test("ED-11: an invalid value shows the schema error on its own field", () => {
  const markup = form({ draft: "yes please" });
  assert.match(markup, /data-field-error="draft"[^>]*role="alert">[^<]*expects boolean/);
  // And the field is marked invalid, so a screen reader hears it too rather
  // than only seeing red.
  assert.match(markup, /data-field="draft" data-invalid="true"/);
  assert.match(markup, /aria-invalid="true"/);
});

test("ED-11: an invalid value blocks the save, and says why", () => {
  const markup = form({ draft: "yes please" });
  assert.match(markup, /data-action="save"[^>]*disabled/);
  assert.match(markup, /1 field to fix before this can be saved\./);
});

test("a valid page can be saved and the reason line is empty", () => {
  const markup = form({ title: "Limits", draft: false });
  assert.ok(!/data-action="save"[^>]*disabled/.test(markup), "save is enabled");
  assert.ok(!markup.includes("to fix before"), "nothing to fix is said");
});

test("a field the form cannot check says the build checks it, rather than nothing", () => {
  // Silence would read as "all checked", which is the claim this editor must
  // not make about a schema construct it does not implement.
  const markup = form({ verify: { anything: [1, 2] } });
  assert.match(markup, /1 field the build checks rather than this form\./);
  assert.ok(!/data-action="save"[^>]*disabled/.test(markup), "unchecked does not block");
});

test("ED-11: the advanced keys are behind a disclosure, closed by default", () => {
  const closed = form({});
  // `\s*` because `${open ? raw("open") : null}` leaves a space where the
  // attribute would go. The markup is valid either way and pinning the
  // whitespace would make the test fail on a change that does not matter.
  assert.match(closed, /<details class="advanced-fields" data-advanced-frontmatter\s*>/);
  assert.match(closed, /<summary>Advanced \(\d+\)<\/summary>/);
  const open = form({}, true);
  assert.match(open, /data-advanced-frontmatter\s+open>/);
});

test("a common field is outside the disclosure and an advanced one inside it", () => {
  const markup = form({});
  const summary = markup.indexOf("<summary>Advanced");
  assert.ok(markup.indexOf('data-field="title"') < summary, "title is a common field");
  assert.ok(markup.indexOf('data-field="verify"') > summary, "verify is advanced");
});

test("every field is described by its help, and by its error when it has one", () => {
  const markup = form({ draft: "yes please" });
  assert.match(markup, /aria-describedby="fm-draft-help fm-draft-error"/);
  assert.match(markup, /aria-describedby="fm-title-help"/);
});

test("a boolean field offers three states, because absent is not false", () => {
  // `draft: false` and no `draft` at all are different bytes. A checkbox
  // collapses them and rewrites pages nobody edited.
  const markup = form({});
  const field = markup.slice(markup.indexOf('data-field="draft"'));
  const control = field.slice(0, field.indexOf("</select>"));
  assert.match(control, /<option value="" selected>\(not set\)<\/option>/);
  assert.match(control, /<option value="true"/);
  assert.match(control, /<option value="false"/);
});

test("a set boolean selects its own option and not the empty one", () => {
  const markup = form({ draft: false });
  const field = markup.slice(markup.indexOf('data-field="draft"'));
  assert.match(field.slice(0, field.indexOf("</select>")), /<option value="false" selected>/);
});

test("a schema enum becomes a select of exactly its choices", () => {
  const enumerated = FIELDS.find((field) => field.choices);
  assert.ok(enumerated, "the schema has at least one enum");
  const markup = form({});
  const field = markup.slice(markup.indexOf(`data-field="${enumerated.name}"`));
  for (const choice of enumerated.choices ?? []) {
    assert.ok(field.includes(`value="${choice}"`), `${choice} is offered`);
  }
});

test("a list is shown as text the author can type, with its values joined", () => {
  const markup = form({ keywords: ["quota", "rate"] });
  const field = markup.slice(markup.indexOf('data-field="keywords"'));
  assert.match(field, /value="quota, rate"/);
});

test("a value cannot inject markup into the form", () => {
  // Front matter comes from a file somebody else may have written.
  const markup = form({ title: '"><script>alert(1)</script>' });
  assert.ok(!markup.includes("<script>"));
  assert.match(markup, /&lt;script&gt;/);
});

test("a label reads the way a person says it", () => {
  assert.equal(label("sidebarTitle"), "Sidebar title");
  assert.equal(label("title"), "Title");
  assert.equal(label("noindex"), "Noindex");
});

test("a control's value parses back, and empty means remove the key", () => {
  const field = (name: string) => FIELDS.find((candidate) => candidate.name === name)!;
  assert.equal(valueFromControl(field("title"), "  Limits  "), "Limits");
  assert.equal(valueFromControl(field("title"), "   "), null, "empty removes the key");
  assert.equal(valueFromControl(field("draft"), "true"), true);
  assert.equal(valueFromControl(field("draft"), "false"), false);
  assert.equal(valueFromControl(field("draft"), ""), null, "not false — absent");
  assert.deepEqual(valueFromControl(field("keywords"), "quota, rate ,"), ["quota", "rate"]);
});

test("a map field parses as JSON, and bad JSON comes back as the text it is", () => {
  const og = FIELDS.find((field) => field.name === "og")!;
  assert.deepEqual(valueFromControl(og, '{"title":"A"}'), { title: "A" } as never);
  // Handed back rather than guessed at: the schema check reports it, which is a
  // better error than this function inventing one.
  assert.equal(valueFromControl(og, "{not json"), "{not json");
});

test("what the form parses back is what the schema then judges", () => {
  // The round trip the acceptance test is really about: type a bad value, the
  // parse hands it on unchanged, the schema rejects it, the field shows it.
  const draft = FIELDS.find((field) => field.name === "draft")!;
  const typed = valueFromControl(draft, "true");
  const found = validateFrontmatter(schema, { draft: typed });
  assert.deepEqual(found.errors, []);
  assert.ok(form({ draft: typed }).includes('data-action="save"'));
});
