// ED-11's front matter form, as markup.
//
// The whole requirement is a form: "front matter is edited through a form
// generated from the schema with per-field help; advanced keys under a
// disclosure", and its acceptance test is that an invalid value shows the
// schema error on its field and blocks the save. So the fields come from
// `formFields(schema)` rather than from a list written here — a form with a
// field the schema does not have writes front matter the build rejects, and
// the author finds out at the next build.
//
// Pure, like every renderer in this package: state in, `Fragment` out.
// `editor.ts` is the only module that puts one in the document.

import { html, raw } from "../escape.ts";
import type { Fragment } from "../escape.ts";
import type { FieldValue, FormField, Validation } from "../frontmatter.ts";

export interface FormState {
  fields: FormField[];
  values: Record<string, FieldValue>;
  validation: Validation;
  /** Whether the advanced disclosure is open. */
  advanced: boolean;
}

/**
 * The form.
 *
 * `aria-describedby` ties each field to its help *and* its error, so a screen
 * reader reads both when the field takes focus. A visible error the field is
 * not described by is an error only sighted users get.
 */
export function renderFrontmatterForm(state: FormState): Fragment {
  const errors = new Map(state.validation.errors.map((error) => [error.field, error.message]));
  const common = state.fields.filter((field) => !field.advanced);
  const advanced = state.fields.filter((field) => field.advanced);

  return html`<form class="frontmatter" data-frontmatter novalidate>
    <h2>Page settings</h2>
    ${common.map((field) => renderField(field, state.values[field.name], errors.get(field.name)))}

    <details class="advanced-fields" data-advanced-frontmatter ${state.advanced ? raw("open") : null}>
      <summary>Advanced (${advanced.length})</summary>
      ${advanced.map((field) => renderField(field, state.values[field.name], errors.get(field.name)))}
    </details>

    ${renderSaveRow(state.validation)}
  </form>`;
}

/**
 * The save control.
 *
 * Disabled is not enough on its own: a disabled button tells somebody nothing
 * about *why*, so the reason sits beside it in a live region and the button is
 * described by it.
 */
function renderSaveRow(validation: Validation): Fragment {
  const blocked = !validation.canSave;
  const unchecked = validation.unchecked.length;
  return html`<div class="form-actions">
    <button
      type="button"
      class="primary"
      data-action="save"
      aria-describedby="frontmatter-save-reason"
      ${blocked ? raw("disabled") : null}
    >Save</button>
    <p class="save-reason" id="frontmatter-save-reason" role="status" aria-live="polite">
      ${blocked
        ? `${validation.errors.length} field${validation.errors.length === 1 ? "" : "s"} to fix before this can be saved.`
        : null}
      ${
        // Said out loud rather than left implicit: the form checked what it can
        // and the build checks the rest. Silence here would read as "all
        // checked", which is the claim this editor must not make.
        unchecked > 0
          ? `${unchecked} field${unchecked === 1 ? "" : "s"} the build checks rather than this form.`
          : null
      }
    </p>
  </div>`;
}

function renderField(field: FormField, value: FieldValue | undefined, error: string | undefined): Fragment {
  const id = `fm-${field.name}`;
  const helpId = `${id}-help`;
  const errorId = `${id}-error`;
  const describedBy = error === undefined ? helpId : `${helpId} ${errorId}`;

  return html`<div class="field" data-field="${field.name}" ${error === undefined ? null : raw('data-invalid="true"')}>
    <label for="${id}">${label(field.name)}</label>
    ${control(field, id, value, describedBy, error !== undefined)}
    <p class="field-help" id="${helpId}" data-field-help>${field.help}</p>
    ${error === undefined
      ? null
      : html`<p class="field-error" id="${errorId}" data-field-error="${field.name}" role="alert">${error}</p>`}
  </div>`;
}

function control(
  field: FormField,
  id: string,
  value: FieldValue | undefined,
  describedBy: string,
  invalid: boolean,
): Fragment {
  const shared = raw(
    `id="${id}" name="${field.name}" aria-describedby="${describedBy}"${invalid ? ' aria-invalid="true"' : ""}`,
  );

  if (field.choices) {
    return html`<select ${shared}>
      <option value="">(not set)</option>
      ${field.choices.map(
        (choice) => html`<option value="${choice}" ${choice === value ? raw("selected") : null}>${choice}</option>`,
      )}
    </select>`;
  }

  switch (field.control) {
    case "boolean":
      // Three states, not two: a front matter key that is absent means "take
      // the project's default", and a checkbox cannot say that. `draft: false`
      // and no `draft` at all are different bytes and a form that collapsed
      // them would rewrite pages nobody edited.
      return html`<select ${shared}>
        <option value="" ${value === undefined || value === null ? raw("selected") : null}>(not set)</option>
        <option value="true" ${value === true ? raw("selected") : null}>Yes</option>
        <option value="false" ${value === false ? raw("selected") : null}>No</option>
      </select>`;
    case "number":
      return html`<input type="number" ${shared} value="${value ?? ""}" />`;
    case "textarea":
      return html`<textarea ${shared} rows="3">${value ?? ""}</textarea>`;
    case "list":
      return html`<input
        type="text"
        ${shared}
        value="${Array.isArray(value) ? value.join(", ") : ""}"
        placeholder="one, then another"
      />`;
    case "object":
    case "opaque":
      // Not editable as a form: a map or a key whose schema this form cannot
      // model is shown as the source it is, in a field the author can still
      // type into, rather than hidden or silently dropped.
      return html`<textarea ${shared} rows="2" data-opaque>${value === undefined ? "" : JSON.stringify(value)}</textarea>`;
    default:
      return html`<input type="text" ${shared} value="${value ?? ""}" />`;
  }
}

/** `sidebarTitle` reads as "Sidebar title" to somebody who does not write code. */
export function label(name: string): string {
  const spaced = name.replace(/([a-z0-9])([A-Z])/g, "$1 $2").toLowerCase();
  return spaced.charAt(0).toUpperCase() + spaced.slice(1);
}

/**
 * What the form's controls parse back to.
 *
 * The inverse of `control` above, and the reason the boolean field has three
 * options: `""` has to come back as "remove this key", not as `false`.
 */
export function valueFromControl(field: FormField, raw: string): FieldValue | null {
  const trimmed = raw.trim();
  if (trimmed === "") return null;
  switch (field.control) {
    case "boolean":
      return trimmed === "true";
    case "number": {
      const parsed = Number(trimmed);
      return Number.isFinite(parsed) ? parsed : trimmed;
    }
    case "list":
      return trimmed
        .split(",")
        .map((part) => part.trim())
        .filter((part) => part !== "");
    case "object":
    case "opaque":
      try {
        return JSON.parse(trimmed) as FieldValue;
      } catch {
        // Handed back as the string it is. The schema check reports it, which
        // is a better error than this function inventing one.
        return trimmed;
      }
    default:
      return trimmed;
  }
}
