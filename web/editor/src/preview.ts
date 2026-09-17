// ED-05: the preview the editor draws beside a chip or a logic block.
//
// Two things here are the editor's own and are deliberately not the build's.
//
// The **caps**: `editor.preview.maxIterations` and `editor.preview.maxBytes`
// bound what is *drawn while someone is typing*, not what a build may produce.
// A page with nine hundred rows is a legitimate page; drawing nine hundred rows
// on every keystroke is not. The build's own caps still apply inside the
// WebAssembly module (`Budget::BUILD`, per WP-24a), so a template that runs
// away is refused there and reported as a diagnostic — these two only decide
// how much of a *successful* expansion reaches the screen.
//
// The **context**: the toolbar picks a version, a locale and sample reader
// groups, and this module assembles the object the build's own
// `Layers::values` would have assembled from them, so a preview answers the
// question "what will readers in this group see", not "what does the editor
// think".

import type { Diagnostic, ExpansionRecord } from "../../../crates/liyasa-wasm/ts/liyasa-wasm.d.ts";
import { isRecord } from "./text.ts";

export interface PreviewLimits {
  maxIterations: number;
  maxBytes: number;
}

/** ED-05's numbers: 50 rows and 256 KB. */
export const PREVIEW_DEFAULTS: PreviewLimits = { maxIterations: 50, maxBytes: 256 * 1024 };

export interface PreviewToolbar {
  version?: string;
  locale?: string;
  product?: string;
  region?: string;
  readerGroups: string[];
}

/** The layers the host already holds for the open draft. */
export interface SiteLayers {
  variables?: Record<string, unknown>;
  facts?: Record<string, unknown>;
  page?: Record<string, unknown>;
  site?: Record<string, unknown>;
  nav?: unknown;
  env?: Record<string, unknown>;
}

const UNITS: Record<string, number> = { B: 1, KB: 1024, MB: 1024 * 1024, GB: 1024 * 1024 * 1024 };
const SIZE = /^(\d+(?:\.\d+)?)\s*(B|KB|MB|GB)$/;

/**
 * The caps in force, and what was wrong with the configuration.
 *
 * A value the schema would reject falls back to the default *and* reports
 * `E0102`. Falling back quietly means the author configured a cap, the editor
 * used a different one, and nothing said so.
 */
export function previewLimits(config: unknown): { limits: PreviewLimits; diagnostics: Diagnostic[] } {
  const diagnostics: Diagnostic[] = [];
  const preview = pathOf(config, ["editor", "preview"]);
  const limits: PreviewLimits = { ...PREVIEW_DEFAULTS };
  if (!isRecord(preview)) return { limits, diagnostics };

  const iterations = preview["maxIterations"];
  if (iterations !== undefined) {
    if (typeof iterations === "number" && Number.isInteger(iterations) && iterations > 0) {
      limits.maxIterations = iterations;
    } else {
      diagnostics.push(schemaError("editor.preview.maxIterations", "a whole number above zero", iterations));
    }
  }

  const bytes = preview["maxBytes"];
  if (bytes !== undefined) {
    const parsed = typeof bytes === "string" ? parseSize(bytes) : null;
    if (parsed === null) {
      diagnostics.push(schemaError("editor.preview.maxBytes", "a byte size such as `256KB`", bytes));
    } else {
      limits.maxBytes = parsed;
    }
  }

  return { limits, diagnostics };
}

function parseSize(written: string): number | null {
  const found = SIZE.exec(written.trim());
  if (!found) return null;
  const unit = UNITS[(found[2] as string).toUpperCase()];
  if (unit === undefined) return null;
  return Math.round(Number(found[1]) * unit);
}

function schemaError(key: string, expected: string, found: unknown): Diagnostic {
  return {
    code: "E0102",
    severity: "error",
    message: `\`${key}\` expects ${expected}, found ${JSON.stringify(found)}; the editor used its default`,
    url: "https://kasecrab.github.io/liyasa/docs/errors/E0102",
  };
}

export interface CappedRows<T> {
  shown: T[];
  total: number;
  truncated: boolean;
}

/**
 * The rows a loop preview draws.
 *
 * `total` is the whole expansion, not the drawn part: "50 of 50" and "50 of
 * 900" are different statements, and the author needs the second to know the
 * preview is partial. It is also what the "show all" control counts.
 */
export function capIterations<T>(rows: T[], limits: PreviewLimits): CappedRows<T> {
  if (rows.length <= limits.maxIterations) {
    return { shown: [...rows], total: rows.length, truncated: false };
  }
  return { shown: rows.slice(0, limits.maxIterations), total: rows.length, truncated: true };
}

export interface CappedText {
  text: string;
  bytes: number;
  truncated: boolean;
}

/**
 * The preview text within the byte cap.
 *
 * The cut lands on a character boundary. Cutting a UTF-8 sequence in half puts
 * a replacement character on screen and makes the editor look like it
 * corrupted the page it is previewing.
 */
export function capBytes(text: string, limits: PreviewLimits): CappedText {
  const encoder = new TextEncoder();
  const total = encoder.encode(text).length;
  if (total <= limits.maxBytes) return { text, bytes: total, truncated: false };

  let kept = "";
  let used = 0;
  for (const character of text) {
    const size = encoder.encode(character).length;
    if (used + size > limits.maxBytes) break;
    kept += character;
    used += size;
  }
  return { text: kept, bytes: used, truncated: true };
}

/**
 * The context object a preview expands against.
 *
 * Shaped the way `liyasa_markdown::source::context::Layers::values` shapes it:
 * a dimension sits at the root, everything else arrives under its own name. A
 * dimension the toolbar did not select is left out rather than sent empty — an
 * empty string is a version named `""`, and `by_version[""]` is a lookup the
 * build would never have made.
 */
export function previewContext(toolbar: PreviewToolbar, layers: SiteLayers): Record<string, unknown> {
  const context: Record<string, unknown> = { ...(layers.variables ?? {}) };
  for (const dimension of ["version", "locale", "product", "region"] as const) {
    const chosen = toolbar[dimension];
    if (chosen !== undefined && chosen !== "") context[dimension] = chosen;
  }
  for (const [name, layer] of [
    ["facts", layers.facts],
    ["page", layers.page],
    ["site", layers.site],
    ["nav", layers.nav],
    ["env", layers.env],
  ] as const) {
    if (layer !== undefined && layer !== null) context[name] = layer;
  }
  if (toolbar.readerGroups.length > 0) context["reader"] = { groups: [...toolbar.readerGroups] };
  return context;
}

export interface ChipSource {
  kind: "fact" | "env" | "reader" | "dimension" | "expression";
  name: string;
  detail: string;
}

/**
 * ED-05's tooltip: where a chip's value came from.
 *
 * The answer is read from the page's own `ExpansionRecord`, so the tooltip
 * cannot claim a source that the expansion did not record. An expression the
 * record does not mention says exactly that rather than guessing.
 */
export function chipSource(expression: string, record: ExpansionRecord): ChipSource {
  const trimmed = expression.trim();

  const fact = /^fact\(\s*["']([^"']+)["']\s*\)$/.exec(trimmed);
  if (fact && record.facts.includes(fact[1] as string)) {
    return { kind: "fact", name: fact[1] as string, detail: "from facts/" };
  }

  const env = /^env\(\s*["']([^"']+)["']\s*\)$/.exec(trimmed);
  if (env && record.env.includes(env[1] as string)) {
    return { kind: "env", name: env[1] as string, detail: "from build.env" };
  }

  const reader = /^reader\.([\w.]+)$/.exec(trimmed);
  if (reader && record.reader_fields.includes(reader[1] as string)) {
    return { kind: "reader", name: trimmed, detail: "from the reader, per request" };
  }

  if (record.dimensions.includes(trimmed)) {
    return { kind: "dimension", name: trimmed, detail: "from the preview context" };
  }

  return { kind: "expression", name: trimmed, detail: "an expression over the preview context" };
}

function pathOf(value: unknown, path: string[]): unknown {
  let at: unknown = value;
  for (const key of path) {
    if (!isRecord(at)) return undefined;
    at = at[key];
  }
  return at;
}
