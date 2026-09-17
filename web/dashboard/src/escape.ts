// Everything the dashboard renders is a string, and most of it came from a
// request: a route, a search query, a feedback comment, a version name. There
// is one escaper and every interpolation goes through it.
//
// `html` returns a `Fragment` rather than a string, which is the whole point.
// A fragment nests inside another `html` without being escaped again; a plain
// string never does. So markup composes and data cannot, and neither can be
// mistaken for the other by forgetting a call.

/** Markup that has already been escaped. */
export class Fragment {
  // A plain field and an explicit assignment: `readonly value: string` in the
  // parameter list is a TypeScript parameter property, which `build.mjs` and
  // `node --test` both refuse because stripping types cannot produce it.
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
    out += renderValue(values[i]) + (strings[i + 1] ?? "");
  }
  return new Fragment(out);
}

/**
 * Marks a string as markup.
 *
 * Used where a fragment was assembled by concatenation rather than by `html`,
 * which in this package is the SVG marks and nowhere else.
 */
export function raw(value: string): Fragment {
  return new Fragment(value);
}

function renderValue(value: unknown): string {
  if (value === null || value === undefined) return "";
  if (value instanceof Fragment) return value.value;
  if (Array.isArray(value)) return value.map(renderValue).join("");
  return escapeHtml(value);
}
