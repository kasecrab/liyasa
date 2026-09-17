// The editor renders author content — page titles, prop values, diagnostic
// messages, a paste from another application — so there is one escaper and
// every interpolation goes through it.
//
// `html` returns a `Fragment` rather than a string: a fragment nests inside
// another `html` untouched, a string is escaped, so markup composes and data
// cannot be mistaken for it by forgetting a call. `web/dashboard/src/escape.ts`
// is the same shape for the same reason; the bundler concatenates one entry
// graph at a time, so each package declares its own.

/** Markup that has already been escaped. */
export class Fragment {
  // A plain field and an explicit assignment: a TypeScript parameter property
  // is not erasable, and both `build.mjs` and `node --test` strip rather than
  // compile.
  value: string;

  constructor(value: string) {
    this.value = value;
  }

  toString(): string {
    return this.value;
  }
}

/** For text and attribute values alike; `html` uses it on both. */
export function escapeHtml(value: unknown): string {
  return String(value)
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#39;");
}

/**
 * A tagged template that escapes every interpolation.
 *
 * A `Fragment` is inserted as written, an array is joined, `null` and
 * `undefined` are nothing, and everything else is escaped.
 */
export function html(strings: TemplateStringsArray, ...values: unknown[]): Fragment {
  let out = strings[0] ?? "";
  for (let i = 0; i < values.length; i += 1) {
    out += renderFragmentValue(values[i]) + (strings[i + 1] ?? "");
  }
  return new Fragment(out);
}

/** Marks a string as markup, for a fragment assembled by concatenation. */
export function raw(value: string): Fragment {
  return new Fragment(value);
}

function renderFragmentValue(value: unknown): string {
  if (value === null || value === undefined) return "";
  if (value instanceof Fragment) return value.value;
  if (Array.isArray(value)) return value.map(renderFragmentValue).join("");
  return escapeHtml(value);
}
