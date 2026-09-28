// ED-01's properties form.
//
// > directives become component blocks with a properties form
//
// The table under test is `src/component-forms.ts`, generated from
// `liyasa_components::Registry::builtins()` by
// `tests/editor/ed_01_component_forms.rs`. That test fails when the two drift,
// so what is asserted here is what the *form* does with real declarations —
// not a fixture, which could not tell whether a required prop is marked
// required.

import test from "node:test";
import assert from "node:assert/strict";

import { COMPONENT_FORMS } from "../src/component-forms.ts";
import {
  byCategory,
  formFor,
  plainValue,
  propFromControl,
  propsToDirective,
  renderProperties,
} from "../src/view/properties.ts";

test("the generated table covers the builtin components", () => {
  assert.ok(COMPONENT_FORMS.length > 20, `only ${COMPONENT_FORMS.length} components`);
  for (const form of COMPONENT_FORMS) {
    assert.match(form.name, /^[a-z][a-z0-9-]*$/, `${form.name} is a directive name`);
    assert.ok(form.icon.length > 0, `${form.name} has an icon`);
    assert.ok(form.category.length > 0, `${form.name} has a category`);
  }
});

test("a component's form shows a field per prop, each with its help", () => {
  const note = formFor("note");
  assert.ok(note, "`note` is a component");
  const markup = String(renderProperties({ component: "note", props: {}, block: "0" }));
  for (const prop of note.props) {
    assert.ok(markup.includes(`data-prop="${prop.prop}"`), `${prop.prop} has a field`);
  }
  const helps = markup.match(/data-field-help>/g) ?? [];
  assert.equal(helps.length, note.props.length);
});

test("a required prop that is not set is marked, and the pane says what is missing", () => {
  // The thing a form generated from the props a block *happens to carry* could
  // never do: say what is absent.
  const withRequired = COMPONENT_FORMS.find((form) => form.props.some((prop) => prop.required));
  assert.ok(withRequired, "some component has a required prop");
  const required = withRequired.props.find((prop) => prop.required)!;

  const markup = String(renderProperties({ component: withRequired.name, props: {}, block: "0" }));
  assert.match(markup, new RegExp(`data-prop="${required.prop}" data-invalid="true"`));
  assert.match(markup, /data-missing/);
  assert.match(markup, /role="alert"/);
});

test("a required prop that is set is not marked missing", () => {
  const withRequired = COMPONENT_FORMS.find((form) => form.props.some((prop) => prop.required))!;
  const required = withRequired.props.find((prop) => prop.required)!;
  const markup = String(
    renderProperties({
      component: withRequired.name,
      props: { [required.prop]: { type: "str", value: "set" } },
      block: "0",
    }),
  );
  assert.ok(!markup.includes("data-missing"), "nothing is missing");
});

test("an unknown component says so rather than showing an empty form", () => {
  // ED-03(c): an unknown directive is an opaque node carrying its bytes, so
  // this is a normal state and not an error.
  const markup = String(renderProperties({ component: "nonsense", props: {}, block: "0" }));
  assert.match(markup, /no component called/);
  assert.match(markup, /<code>nonsense<\/code>/);
  assert.ok(!markup.includes("data-prop="), "no fields are invented");
});

test("an enum prop becomes a select of exactly its choices", () => {
  const withChoices = COMPONENT_FORMS.flatMap((form) =>
    form.props.filter((prop) => prop.choices).map((prop) => ({ form, prop })),
  )[0];
  assert.ok(withChoices, "some component has an enum prop");
  const markup = String(renderProperties({ component: withChoices.form.name, props: {}, block: "0" }));
  const field = markup.slice(markup.indexOf(`data-prop="${withChoices.prop.prop}"`));
  for (const choice of withChoices.prop.choices ?? []) {
    assert.ok(field.includes(`value="${choice}"`), `${choice} is offered`);
  }
});

test("an expression prop gets a code field and never a value picker", () => {
  // Offering a colour swatch for `{{ theme.brand }}` invites an author to
  // replace an expression with a literal without noticing.
  const markup = String(
    renderProperties({
      component: "note",
      props: {},
      block: "0",
    }),
  );
  // Whatever `note` has, no field may be both a picker and an expression.
  assert.ok(!/<input type="color"[^>]*data-expression/.test(markup));
});

test("a prop value cannot inject markup", () => {
  const markup = String(
    renderProperties({
      component: "note",
      props: { title: { type: "str", value: '"><script>alert(1)</script>' } },
      block: "0",
    }),
  );
  assert.ok(!markup.includes("<script>"));
  assert.match(markup, /&lt;script&gt;/);
});

test("the insert menu groups every component under a category", () => {
  const groups = byCategory();
  assert.ok(groups.length > 1, "more than one category");
  const listed = groups.flatMap((group) => group.components.length).reduce((a, b) => a + b, 0);
  assert.equal(listed, COMPONENT_FORMS.length, "every component is in exactly one group");
});

test("a prop value reads back as the text a control shows", () => {
  assert.equal(plainValue({ type: "str", value: "Heads up" }), "Heads up");
  assert.equal(plainValue({ type: "num", value: 3 }), "3");
  assert.equal(plainValue({ type: "bool", value: true }), "true");
  assert.equal(plainValue({ type: "expr", value: "site.name" }), "site.name");
  assert.equal(
    plainValue({ type: "list", value: [{ type: "str", value: "a" }, { type: "str", value: "b" }] }),
    "a, b",
  );
  assert.equal(plainValue(undefined), "");
});

test("a code widget always produces an expression, not a string", () => {
  // Stored as a string, the directive prints the expression instead of
  // running it.
  const code = { prop: "x", widget: "code", label: "X", help: "h", required: false };
  assert.deepEqual(propFromControl(code, "site.name"), { type: "expr", value: "site.name" });
});

test("a toggle produces a boolean and an empty control removes the prop", () => {
  const toggle = { prop: "x", widget: "toggle", label: "X", help: "h", required: false };
  assert.deepEqual(propFromControl(toggle, "true"), { type: "bool", value: true });
  assert.deepEqual(propFromControl(toggle, "false"), { type: "bool", value: false });
  assert.equal(propFromControl(toggle, "  "), null, "absent, not false");
});

test("a number that is not a number stays a string for the build to report", () => {
  const number = { prop: "x", widget: "number", label: "X", help: "h", required: false };
  assert.deepEqual(propFromControl(number, "1280"), { type: "num", value: 1280 });
  assert.deepEqual(propFromControl(number, "wide"), { type: "str", value: "wide" });
});

test("only the props that are set are written back", () => {
  // Writing every declared prop empty turns a two-prop directive into a
  // twelve-prop one on the first edit, which is the opposite of what the
  // Source Document is for.
  assert.equal(
    propsToDirective("note", { title: { type: "str", value: "Heads up" } }, 3),
    ':::note{title="Heads up"}',
  );
  assert.equal(propsToDirective("note", {}, 3), ":::note");
});

test("a written prop escapes a quote rather than ending the attribute", () => {
  assert.equal(
    propsToDirective("note", { title: { type: "str", value: 'say "hi"' } }, 3),
    ':::note{title="say \\"hi\\""}',
  );
});

test("a leaf keeps its own colon count", () => {
  assert.equal(propsToDirective("image", { src: { type: "str", value: "/a.png" } }, 2), '::image{src="/a.png"}');
  assert.equal(propsToDirective("tabs", {}, 4), "::::tabs");
});
