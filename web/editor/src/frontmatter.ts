// ED-11: front matter, and the form that edits it.
//
// Two things here are deliberate.
//
// **Writing back is line-surgical.** A page's front matter often carries a
// comment, a key order somebody chose, or a quoting style; rewriting the block
// from the parsed map loses all three on the first edit. The Source Document
// exists so that an editor cannot reformat a page nobody edited, and front
// matter is held to the same standard: a changed key replaces its own lines
// and nothing else moves.
//
// **The form comes from `schemas/frontmatter.json`.** That file is generated
// from the Rust types, so a field the form offers is a field the build
// accepts. The help text is this package's own, because the schema carries no
// `description` for most properties — a test keeps the two keyed to the same
// names so they cannot drift without saying so.

import type { Diagnostic } from "../../../crates/liyasa-wasm/ts/liyasa-wasm.d.ts";

export type Scalar = string | number | boolean | null;
export type FieldValue = Scalar | Scalar[] | Record<string, Scalar>;

export interface ParsedFrontmatter {
  /** The `---` block including both fences, or `""` when there is none. */
  raw: string;
  fields: Record<string, FieldValue>;
  body: string;
}

const FENCE = /^---[ \t]*\r?\n/;

export function parseFrontmatter(text: string): ParsedFrontmatter {
  if (!FENCE.test(text)) return { raw: "", fields: {}, body: text };
  const lines = text.split(/(?<=\n)/);
  let close = -1;
  for (let at = 1; at < lines.length; at += 1) {
    if (/^---[ \t]*\r?\n?$/.test(lines[at] as string)) {
      close = at;
      break;
    }
  }
  // An unterminated block is not front matter; treating it as one would eat
  // the page.
  if (close === -1) return { raw: "", fields: {}, body: text };

  const raw = lines.slice(0, close + 1).join("");
  const body = lines.slice(close + 1).join("");
  return { raw, fields: readFields(lines.slice(1, close)), body };
}

function readFields(lines: string[]): Record<string, FieldValue> {
  const fields: Record<string, FieldValue> = {};
  let at = 0;
  while (at < lines.length) {
    const line = (lines[at] as string).replace(/\r?\n$/, "");
    at += 1;
    if (line.trim() === "" || line.trimStart().startsWith("#")) continue;
    const match = /^([A-Za-z_][\w-]*)\s*:\s*(.*)$/.exec(line);
    if (!match) continue;
    const key = match[1] as string;
    const inline = (match[2] as string).trim();
    if (inline !== "") {
      fields[key] = readScalar(inline);
      continue;
    }
    // A key with nothing after the colon opens a list or a map.
    const nested: string[] = [];
    while (at < lines.length && /^\s+\S/.test((lines[at] as string).replace(/\r?\n$/, ""))) {
      nested.push((lines[at] as string).replace(/\r?\n$/, ""));
      at += 1;
    }
    if (nested.length === 0) {
      fields[key] = null;
    } else if (nested.every((entry) => /^\s*-\s/.test(entry))) {
      fields[key] = nested.map((entry) => readScalar(entry.replace(/^\s*-\s*/, "")));
    } else {
      const map: Record<string, Scalar> = {};
      for (const entry of nested) {
        const pair = /^\s*([A-Za-z_][\w-]*)\s*:\s*(.*)$/.exec(entry);
        if (pair) map[pair[1] as string] = readScalar((pair[2] as string).trim());
      }
      fields[key] = map;
    }
  }
  return fields;
}

function readScalar(written: string): Scalar {
  if (written === "" || written === "null" || written === "~") return null;
  const quoted = /^"(.*)"$|^'(.*)'$/.exec(written);
  if (quoted) return quoted[1] ?? quoted[2] ?? "";
  if (written === "true") return true;
  if (written === "false") return false;
  if (/^-?\d+$/.test(written)) return Number(written);
  if (/^-?\d+\.\d+$/.test(written)) return Number(written);
  return written;
}

const NEEDS_QUOTES = /^[\s>|&*!%@`{[]|:\s|\s$|^$|^(true|false|null|~|-?\d+(\.\d+)?)$|#/;

function writeScalar(value: Scalar): string {
  if (value === null) return "";
  if (typeof value !== "string") return String(value);
  if (NEEDS_QUOTES.test(value)) return `"${value.replace(/"/g, '\\"')}"`;
  return value;
}

function writeEntry(key: string, value: FieldValue): string {
  if (Array.isArray(value)) {
    if (value.length === 0) return `${key}: []\n`;
    return `${key}:\n${value.map((item) => `  - ${writeScalar(item)}\n`).join("")}`;
  }
  if (value !== null && typeof value === "object") {
    const pairs = Object.entries(value);
    if (pairs.length === 0) return `${key}: {}\n`;
    return `${key}:\n${pairs.map(([name, item]) => `  ${name}: ${writeScalar(item)}\n`).join("")}`;
  }
  return `${key}: ${writeScalar(value)}\n`;
}

/**
 * The page with `changes` applied to its front matter.
 *
 * A key set to `null` is removed. A key the block does not have is appended
 * before the closing fence. Every other line — comments, key order, quoting —
 * is copied through.
 */
export function writeFrontmatter(text: string, changes: Record<string, FieldValue | null>): string {
  const parsed = parseFrontmatter(text);
  if (parsed.raw === "") {
    const written = Object.entries(changes)
      .filter(([, value]) => value !== null)
      .map(([key, value]) => writeEntry(key, value as FieldValue))
      .join("");
    return written === "" ? text : `---\n${written}---\n\n${text.replace(/^\n+/, "")}`;
  }

  const lines = parsed.raw.split(/(?<=\n)/);
  const close = lines.length - 1;
  const out: string[] = [lines[0] as string];
  const applied = new Set<string>();

  for (let at = 1; at < close; at += 1) {
    const line = lines[at] as string;
    const match = /^([A-Za-z_][\w-]*)\s*:/.exec(line);
    const key = match?.[1];
    if (key === undefined || !(key in changes)) {
      out.push(line);
      continue;
    }
    applied.add(key);
    const value = changes[key] as FieldValue | null;
    if (value !== null) out.push(writeEntry(key, value));
    // Whether replaced or removed, the key's continuation lines go with it.
    while (at + 1 < close && /^\s+\S/.test(lines[at + 1] as string)) at += 1;
  }

  for (const [key, value] of Object.entries(changes)) {
    if (applied.has(key) || value === null) continue;
    out.push(writeEntry(key, value));
  }
  out.push(lines[close] as string);
  return out.join("") + parsed.body;
}

// --- the form ---------------------------------------------------------------

/** What an author sets on most pages; everything else sits under a disclosure. */
const COMMON = new Set(["title", "description", "sidebarTitle", "icon", "tag", "draft"]);

/**
 * Help for every property of `schemas/frontmatter.json`.
 *
 * ED-11 asks for per-field help and the schema has a `description` for four
 * properties out of thirty-eight, so this is the editor's copy. The test that
 * walks both directions is what keeps it honest when the Rust types change.
 */
export const FIELD_HELP: Record<string, string> = {
  access: "Who may read this page. Readers outside the rule are served as if it did not exist.",
  ai: "Whether assistants may use this page, and how it is summarised for them.",
  asyncapi: "An AsyncAPI operation this page documents, as `spec-id channel`.",
  authors: "Author keys from `authors` in liyasa.json, shown on the page.",
  canonical: "The URL search engines should treat as the original of this page.",
  date: "The day this page was first published, as `YYYY-MM-DD`.",
  description: "One sentence for search results, link previews and the page header.",
  draft: "Keep this page out of a production build. It still builds in `liyasa dev`.",
  facts: "Values this page defines for itself, readable as `facts.*` in templates.",
  graphql: "A GraphQL operation this page documents, as `spec-id Operation`.",
  groups: "Access groups that may see this page.",
  hidden: "Keep the page routable but out of the navigation.",
  icon: "Icon shown beside the page in the sidebar.",
  iconType: "Which icon set `icon` names, when it is not the default.",
  id: "The page's permanent identifier. It survives renames; changing it breaks every link that used it.",
  keywords: "Extra words search should match this page on.",
  locales: "Locales this page exists in. Absent means every locale.",
  mode: "The page's layout: the default, wide, or a custom template.",
  noindex: "Ask search engines not to index this page.",
  og: "Open Graph title, description and image for link previews.",
  openapi: "An API operation this page documents, as `spec-id METHOD /path`.",
  personalized: "This page reads `reader.*` fields and is rendered per request rather than at build time.",
  product: "The product this page belongs to, when the site has more than one.",
  regions: "Regions this page belongs to; a reader outside them does not see it.",
  related: "Pages shown as related topics, by page id or route.",
  reviewed: "When this page was last reviewed, as `YYYY-MM-DD`.",
  search: "Whether search indexes this page, and how much it is boosted.",
  sidebarTitle: "A shorter title for the sidebar, when the page title is long.",
  slug: "The last segment of the URL. It stays put when the title changes.",
  tag: "A short badge beside the page in the sidebar, such as `new` or `beta`.",
  template: "A page template to render this page with.",
  title: "The page's heading, its sidebar entry, and its browser tab.",
  twitter: "Twitter card title, description and image.",
  updated: "When this page last changed, as `YYYY-MM-DD`.",
  url: "An external link. The page becomes a navigation entry with no body of its own.",
  variation: "Content variations this page belongs to.",
  verify: "Verification rules for this page: what must be true for it to build green.",
  versions: "Versions this page exists in. Absent means every version.",
};

export const ADVANCED = new Set(Object.keys(FIELD_HELP).filter((name) => !COMMON.has(name)));

export type FieldControl = "text" | "textarea" | "boolean" | "number" | "list" | "object" | "opaque";

export interface FormField {
  name: string;
  control: FieldControl;
  advanced: boolean;
  help: string;
  /** Present when the schema fixes the value set. */
  choices?: string[];
}

interface JsonSchema {
  properties: Record<string, Record<string, unknown>>;
  $defs?: Record<string, Record<string, unknown>>;
}

/** One field per schema property, in the schema's own order. */
export function formFields(schema: JsonSchema): FormField[] {
  return Object.entries(schema.properties).map(([name, property]) => {
    const field: FormField = {
      name,
      control: controlFor(name, property),
      advanced: ADVANCED.has(name),
      help: FIELD_HELP[name] ?? "",
    };
    const choices = resolved(schema, property)["enum"];
    if (Array.isArray(choices)) field.choices = choices.map(String);
    return field;
  });
}

function controlFor(name: string, property: Record<string, unknown>): FieldControl {
  const types = typesOf(property);
  if (types.includes("boolean")) return "boolean";
  if (types.includes("integer") || types.includes("number")) return "number";
  if (types.includes("array")) return "list";
  if (types.includes("object")) return "object";
  if (types.includes("string")) return name === "description" ? "textarea" : "text";
  return "opaque";
}

function typesOf(property: Record<string, unknown>): string[] {
  const type = property["type"];
  if (typeof type === "string") return [type];
  if (Array.isArray(type)) return type.map(String);
  return [];
}

function resolved(schema: JsonSchema, property: Record<string, unknown>): Record<string, unknown> {
  const anyOf = property["anyOf"];
  if (!Array.isArray(anyOf)) return property;
  for (const branch of anyOf) {
    if (typeof branch !== "object" || branch === null) continue;
    const reference = (branch as Record<string, unknown>)["$ref"];
    if (typeof reference !== "string") continue;
    const name = reference.replace("#/$defs/", "");
    const target = schema.$defs?.[name];
    if (target) return target;
  }
  return property;
}

export interface FieldError {
  field: string;
  code: string;
  message: string;
}

export interface Validation {
  errors: FieldError[];
  /** Fields whose schema this validator does not implement. */
  unchecked: string[];
  canSave: boolean;
}

/**
 * ED-11's save gate.
 *
 * What it does not check it says it did not check. A validator that reports
 * "valid" for a value nothing looked at is the editor asserting something it
 * never established; the build still validates everything, and an unchecked
 * field does not block a save.
 */
export function validateFrontmatter(schema: JsonSchema, fields: Record<string, unknown>): Validation {
  const errors: FieldError[] = [];
  const unchecked: string[] = [];

  for (const [name, value] of Object.entries(fields)) {
    const property = schema.properties[name];
    if (!property) {
      errors.push({
        field: name,
        code: "E0102",
        message: `\`${name}\` is not a front matter key. The keys this project accepts are in schemas/frontmatter.json.`,
      });
      continue;
    }
    if (value === null || value === undefined) continue;

    const types = typesOf(property).filter((type) => type !== "null");
    if (types.length === 0) {
      unchecked.push(name);
      continue;
    }
    if (!types.some((type) => matches(type, value))) {
      errors.push({
        field: name,
        code: "E0102",
        message: `\`${name}\` expects ${types.join(" or ")}, found ${describe(value)}.`,
      });
      continue;
    }
    if (types.includes("array") && Array.isArray(value)) {
      const items = property["items"];
      const itemType = isRecord(items) ? items["type"] : undefined;
      if (typeof itemType !== "string") {
        unchecked.push(name);
        continue;
      }
      if (!value.every((item) => matches(itemType, item))) {
        errors.push({
          field: name,
          code: "E0102",
          message: `every entry of \`${name}\` must be ${itemType}.`,
        });
      }
    }
  }

  return { errors, unchecked, canSave: errors.length === 0 };
}

/** The same findings as diagnostics, for the pane that lists them. */
export function frontmatterDiagnostics(validation: Validation): Diagnostic[] {
  return validation.errors.map((error) => ({
    code: error.code,
    severity: "error",
    message: error.message,
    url: `https://kasecrab.github.io/liyasa/docs/errors/${error.code}`,
  }));
}

function matches(type: string, value: unknown): boolean {
  switch (type) {
    case "string":
      return typeof value === "string";
    case "boolean":
      return typeof value === "boolean";
    case "integer":
      return typeof value === "number" && Number.isInteger(value);
    case "number":
      return typeof value === "number";
    case "array":
      return Array.isArray(value);
    case "object":
      return isRecord(value);
    case "null":
      return value === null;
    default:
      return false;
  }
}

function describe(value: unknown): string {
  if (Array.isArray(value)) return "a list";
  if (value === null) return "null";
  if (isRecord(value)) return "a map";
  return typeof value;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
