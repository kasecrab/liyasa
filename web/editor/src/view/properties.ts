// ED-01's properties form: the pane a component block opens.
//
// > directives become component blocks with a properties form
//
// The fields come from `COMPONENT_FORMS`, generated from
// `liyasa_components::Registry::builtins()` — every component's `editor_block()`
// for the widget, label and help, and its `PropSchema` for `required` and an
// enum's choices. A form that invented its fields from the props a block
// happens to carry could never say what is *missing*, which is most of what a
// properties form is for.
//
// Pure: state in, `Fragment` out. `editor.ts` puts it in the document.

import { html, raw } from "../escape.ts";
import type { Fragment } from "../escape.ts";
import { COMPONENT_FORMS } from "../component-forms.ts";
import type { ComponentForm, ComponentProp } from "../component-forms.ts";
import type { PropValue } from "../../../crates/liyasa-wasm/ts/liyasa-wasm.d.ts";

export function formFor(name: string): ComponentForm | undefined {
  return COMPONENT_FORMS.find((form) => form.name === name);
}

/** Every component, grouped the way the insert menu groups them. */
export function byCategory(): { category: string; components: ComponentForm[] }[] {
  const groups = new Map<string, ComponentForm[]>();
  for (const form of COMPONENT_FORMS) {
    groups.set(form.category, [...(groups.get(form.category) ?? []), form]);
  }
  return [...groups.entries()]
    .sort((left, right) => left[0].localeCompare(right[0]))
    .map(([category, components]) => ({ category, components }));
}

export interface PropertiesState {
  /** The directive name on the block under the cursor. */
  component: string;
  props: Record<string, PropValue>;
  /** The block id, so an edit knows what it is editing. */
  block: string;
}

/**
 * The properties pane.
 *
 * A component the registry does not know is not an error here — ED-03(c) says
 * an unknown directive is an opaque node carrying its bytes — so the pane says
 * so and offers the source instead of an empty form.
 */
export function renderProperties(state: PropertiesState): Fragment {
  const form = formFor(state.component);
  if (!form) {
    return html`<section class="properties" data-properties="${state.block}">
      <h2>${state.component}</h2>
      <p class="empty">
        This project has no component called <code>${state.component}</code>, so there are no
        settings to show. Its source is editable in the block itself.
      </p>
    </section>`;
  }

  const missing = form.props.filter(
    (prop) => prop.required && state.props[prop.prop] === undefined,
  );

  return html`<section class="properties" data-properties="${state.block}" data-component="${form.name}">
    <h2>${form.name}</h2>
    ${missing.length === 0
      ? null
      : html`<p class="missing" role="alert" data-missing>
          ${missing.length === 1
            ? `This ${form.name} needs a ${missing[0]?.label.toLowerCase()}.`
            : `This ${form.name} needs ${missing.length} settings it does not have.`}
        </p>`}
    ${form.props.map((prop) => renderPropField(form.name, prop, state.props[prop.prop]))}
    ${form.slots.length === 0 ? null : renderSlots(form)}
  </section>`;
}

function renderSlots(form: ComponentForm): Fragment {
  return html`<details class="slots">
    <summary>Parts (${form.slots.length})</summary>
    <ul>
      ${form.slots.map(
        (slot) => html`<li data-slot="${slot.name}">
          <strong>${slot.name}</strong>${slot.required ? " (required)" : null}
          <span class="field-help">${slot.help}</span>
        </li>`,
      )}
    </ul>
  </details>`;
}

function renderPropField(component: string, prop: ComponentProp, value: PropValue | undefined): Fragment {
  const id = `prop-${component}-${prop.prop}`;
  const helpId = `${id}-help`;
  const unset = value === undefined;
  const invalid = prop.required && unset;

  return html`<div class="field" data-prop="${prop.prop}"${invalid ? raw(' data-invalid="true"') : null}>
    <label for="${id}">${prop.label}${prop.required ? html`<span class="required" aria-hidden="true">*</span>` : null}</label>
    ${propControl(prop, id, helpId, value, invalid)}
    <p class="field-help" id="${helpId}" data-field-help>${prop.help}</p>
  </div>`;
}

/**
 * The control a widget asks for.
 *
 * An `expr` prop is a template expression the build evaluates, so it gets a
 * code field and never a value picker — offering a colour swatch for
 * `{{ theme.brand }}` would invite an author to replace an expression with a
 * propLiteral without noticing.
 */
function propControl(
  prop: ComponentProp,
  id: string,
  helpId: string,
  value: PropValue | undefined,
  invalid: boolean,
): Fragment {
  const shared = raw(
    `id="${id}" name="${prop.prop}" aria-describedby="${helpId}"` +
      (invalid ? ' aria-invalid="true" required' : "") +
      (prop.required ? "" : ""),
  );
  const text = plainValue(value);

  if (prop.choices) {
    return html`<select ${shared}>
      ${prop.required ? null : html`<option value="">(not set)</option>`}
      ${prop.choices.map(
        (choice) => html`<option value="${choice}" ${choice === text ? raw("selected") : null}>${choice}</option>`,
      )}
    </select>`;
  }

  switch (prop.widget) {
    case "toggle":
      // Three states again: an absent boolean prop takes the component's own
      // default, which is not the same as `false`.
      return html`<select ${shared}>
        <option value="" ${value === undefined ? raw("selected") : null}>(not set)</option>
        <option value="true" ${text === "true" ? raw("selected") : null}>Yes</option>
        <option value="false" ${text === "false" ? raw("selected") : null}>No</option>
      </select>`;
    case "number":
      return html`<input type="number" ${shared} value="${text}" />`;
    case "color":
      // Text beside the swatch, because a swatch cannot express a token like
      // `var(--ly-color-primary)` and an author who only had the swatch would
      // have to overwrite the token to use the field at all.
      return html`<span class="color-field">
        <input type="color" ${shared} value="${/^#[0-9a-fA-F]{6}$/.test(text) ? text : "#000000"}" />
        <input type="text" name="${prop.prop}-text" value="${text}" aria-label="${prop.label} as text" />
      </span>`;
    case "asset":
      return html`<span class="asset-field">
        <input type="text" ${shared} value="${text}" />
        <button type="button" data-open-media="${prop.prop}">Choose…</button>
      </span>`;
    case "code":
      return html`<textarea ${shared} rows="2" spellcheck="false" data-expression>${text}</textarea>`;
    default:
      return html`<input type="text" ${shared} value="${text}" />`;
  }
}

/** A `PropValue` as the text a control shows. */
export function plainValue(value: PropValue | undefined): string {
  if (value === undefined) return "";
  switch (value.type) {
    case "str":
    case "expr":
      return value.value;
    case "num":
      return String(value.value);
    case "bool":
      return value.value ? "true" : "false";
    case "list":
      return value.value.map((item) => plainValue(item)).join(", ");
  }
}

/**
 * The `PropValue` a control's text becomes, or `null` to remove the prop.
 *
 * A `code` widget always produces an `expr`: its whole purpose is to hold
 * something the build evaluates, and storing it as a string would have the
 * directive print the expression rather than run it.
 */
export function propFromControl(prop: ComponentProp, raw: string): PropValue | null {
  const trimmed = raw.trim();
  if (trimmed === "") return null;
  if (prop.widget === "code") return { type: "expr", value: trimmed };
  if (prop.widget === "toggle") return { type: "bool", value: trimmed === "true" };
  if (prop.widget === "number") {
    const parsed = Number(trimmed);
    // Not a number stays a string, so the build's own prop check reports it
    // with the code and the span rather than this function guessing.
    return Number.isFinite(parsed) ? { type: "num", value: parsed } : { type: "str", value: trimmed };
  }
  return { type: "str", value: trimmed };
}

/**
 * The directive text a set of props serialises to, for one leaf or open line.
 *
 * Only the props that are set are written. Writing every declared prop with an
 * empty value would turn a two-prop directive into a twelve-prop one on the
 * first edit, which is the opposite of what the Source Document is for.
 */
export function propsToDirective(
  name: string,
  props: Record<string, PropValue>,
  colons: number,
): string {
  const written = Object.entries(props)
    .filter(([, value]) => value !== undefined)
    .map(([key, value]) => `${key}=${propLiteral(value)}`);
  const marker = ":".repeat(Math.max(colons, 2));
  return written.length === 0 ? `${marker}${name}` : `${marker}${name}{${written.join(" ")}}`;
}

function propLiteral(value: PropValue): string {
  switch (value.type) {
    case "num":
      return String(value.value);
    case "bool":
      return value.value ? "true" : "false";
    case "expr":
      return `"${value.value.replace(/"/g, '\\"')}"`;
    case "list":
      return `"${value.value.map((item) => plainValue(item)).join(",")}"`;
    default:
      return `"${value.value.replace(/\\/g, "\\\\").replace(/"/g, '\\"')}"`;
  }
}
