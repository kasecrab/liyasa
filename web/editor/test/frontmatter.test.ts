// ED-11: front matter read, written back byte-preservingly, and edited
// through a form generated from `schemas/frontmatter.json`.

import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";
import assert from "node:assert/strict";

import {
  ADVANCED,
  FIELD_HELP,
  formFields,
  parseFrontmatter,
  validateFrontmatter,
  writeFrontmatter,
} from "../src/frontmatter.ts";

const HERE = dirname(fileURLToPath(import.meta.url));
const schema = JSON.parse(readFileSync(resolve(HERE, "../../../schemas/frontmatter.json"), "utf8"));

test("a page with no front matter parses as empty and keeps its body", () => {
  const parsed = parseFrontmatter("# Title\n\nbody\n");
  assert.deepEqual(parsed.fields, {});
  assert.equal(parsed.raw, "");
  assert.equal(parsed.body, "# Title\n\nbody\n");
});

test("scalars, lists and nested maps are read", () => {
  const text = [
    "---",
    "title: Limits",
    "draft: true",
    "order: 3",
    "keywords:",
    "  - quota",
    "  - rate",
    "og:",
    "  title: Social",
    "---",
    "",
    "body\n",
  ].join("\n");
  const parsed = parseFrontmatter(text);
  assert.equal(parsed.fields["title"], "Limits");
  assert.equal(parsed.fields["draft"], true);
  assert.equal(parsed.fields["order"], 3);
  assert.deepEqual(parsed.fields["keywords"], ["quota", "rate"]);
  assert.deepEqual(parsed.fields["og"], { title: "Social" });
  assert.equal(parsed.body, "\nbody\n");
});

test("a quoted value keeps its colons and its quotes do not become part of it", () => {
  const parsed = parseFrontmatter('---\ntitle: "Limits: the caps"\ndate: 2026-09-17\n---\n');
  assert.equal(parsed.fields["title"], "Limits: the caps");
  assert.equal(parsed.fields["date"], "2026-09-17");
});

test("writing back a field the author did not touch changes no other byte", () => {
  // The Source Document exists so that an editor cannot reformat a page
  // nobody edited; front matter is held to the same standard.
  const text = '---\ntitle: Limits\n# a comment nobody should lose\ndescription: "The caps"\n---\n\nbody\n';
  const written = writeFrontmatter(text, { title: "Rate limits" });
  assert.equal(
    written,
    '---\ntitle: Rate limits\n# a comment nobody should lose\ndescription: "The caps"\n---\n\nbody\n',
  );
});

test("a new field is appended rather than the block being rewritten", () => {
  const written = writeFrontmatter("---\ntitle: A\n---\n\nbody\n", { description: "B" });
  assert.equal(written, "---\ntitle: A\ndescription: B\n---\n\nbody\n");
});

test("a value set to null removes its line", () => {
  const written = writeFrontmatter("---\ntitle: A\ndraft: true\n---\n\nbody\n", { draft: null });
  assert.equal(written, "---\ntitle: A\n---\n\nbody\n");
});

test("front matter is created for a page that has none", () => {
  assert.equal(writeFrontmatter("body\n", { title: "A" }), "---\ntitle: A\n---\n\nbody\n");
});

test("a value that needs quoting gets them", () => {
  const written = writeFrontmatter("---\nx: 1\n---\n", { title: "Limits: the caps" });
  assert.match(written, /title: "Limits: the caps"/);
  assert.equal(parseFrontmatter(written).fields["title"], "Limits: the caps");
});

test("the form is generated from the schema, and offers nothing the schema lacks", () => {
  // A form with a field the schema does not have writes front matter the build
  // rejects, and the author finds out at the next build.
  const fields = formFields(schema);
  assert.ok(fields.length > 20, "the form covers the schema");
  for (const field of fields) {
    assert.ok(field.name in schema.properties, `${field.name} is a schema property`);
  }
  const names = new Set(fields.map((field) => field.name));
  for (const property of Object.keys(schema.properties)) {
    assert.ok(names.has(property), `${property} has a form field`);
  }
});

test("every field the form shows has help, and help exists for nothing else", () => {
  // The schema carries no `description` for most front matter properties, so
  // the help is this package's own copy. Keyed by the schema's names so the
  // two cannot drift apart without this test saying so.
  for (const name of Object.keys(schema.properties)) {
    assert.ok(FIELD_HELP[name], `${name} has help text`);
  }
  for (const name of Object.keys(FIELD_HELP)) {
    assert.ok(name in schema.properties, `${name} is still a schema property`);
  }
});

test("the common fields are shown and the rest sit under the disclosure", () => {
  const fields = formFields(schema);
  const shown = fields.filter((field) => !field.advanced).map((field) => field.name);
  assert.deepEqual(shown.sort(), ["description", "draft", "icon", "sidebarTitle", "tag", "title"]);
  assert.ok(ADVANCED.has("verify"), "a whole subtree is advanced");
});

test("a value of the wrong type is a schema error that blocks saving", () => {
  const found = validateFrontmatter(schema, { title: 5, draft: "yes" });
  assert.deepEqual(found.errors.map((error) => error.field).sort(), ["draft", "title"]);
  assert.equal(found.errors[0]?.code, "E0102");
  assert.equal(found.canSave, false);
});

test("a key the schema does not declare is a schema error", () => {
  const found = validateFrontmatter(schema, { nonsense: 1 });
  assert.equal(found.errors[0]?.field, "nonsense");
  assert.equal(found.canSave, false);
});

test("a correct page saves", () => {
  const found = validateFrontmatter(schema, { title: "A", draft: false, keywords: ["x"] });
  assert.deepEqual(found.errors, []);
  assert.equal(found.canSave, true);
});

test("a field whose schema this validator cannot check is reported as unchecked, not as valid", () => {
  // Saying "valid" about a value nothing looked at is the failure this whole
  // ledger is about. `verify` mirrors a schema object several levels deep.
  const found = validateFrontmatter(schema, { verify: { anything: [1, 2] } });
  assert.deepEqual(found.errors, []);
  assert.ok(found.unchecked.includes("verify"));
  assert.equal(found.canSave, true, "unchecked does not block; the build still checks it");
});
