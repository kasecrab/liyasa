// ED-12: find and replace across pages, move a group, apply a tag, change a
// fact reference.
//
// **The preview is the result.** The requirement says the diff preview must
// match what was applied; the only way to guarantee that is for `applyPlan` to
// have nothing left to decide — it writes the `after` text the plan already
// computed. A preview that re-runs the replace can disagree with it, and the
// author has no way to tell which one ran.
//
// **Scope is decided by segment, not by regular expression.** A phrase in
// prose, the same phrase in a directive's prop, and the same phrase inside a
// fenced `curl` command are three different things. Replacing the third
// silently changes a command a reader will run, so `code` is opt-in. A
// template expression is never rewritten at all: `{{ site.name }}` is code the
// build runs, and editing it changes what the page computes, which is not what
// "replace this phrase" means to anybody.

import type { Diagnostic, SegmentEdit, SourceDocument } from "../../../crates/liyasa-wasm/ts/liyasa-wasm.d.ts";
import { parseFrontmatter, writeFrontmatter } from "./frontmatter.ts";
import type { Project, ProjectChange, NavigationGroup } from "./pages.ts";
import { navigationOf } from "./pages.ts";

export interface ScannedPage {
  path: string;
  source: string;
  document: SourceDocument;
}

/** Which segments a replace is allowed to touch. */
export type Scope = "text" | "props" | "code";

export const DEFAULT_SCOPES: Scope[] = ["text", "props"];

export interface Match {
  path: string;
  segment: number;
  /** 1-based, in the whole file. */
  line: number;
  column: number;
  /** The line the author sees in the preview. */
  text: string;
}

export interface PlannedPage {
  path: string;
  before: string;
  after: string;
  edits: SegmentEdit[];
}

export interface BulkPlan {
  matches: Match[];
  pages: PlannedPage[];
  diagnostics: Diagnostic[];
}

export interface ReplaceOptions {
  find: string;
  replaceWith: string;
  scopes?: Scope[];
  regex?: boolean;
  caseSensitive?: boolean;
}

export function findReplace(pages: ScannedPage[], options: ReplaceOptions): BulkPlan {
  const scopes = new Set(options.scopes ?? DEFAULT_SCOPES);
  let pattern: RegExp;
  try {
    const body = options.regex ? options.find : escapeRegExp(options.find);
    pattern = new RegExp(body, options.caseSensitive === false ? "gi" : "g");
  } catch (cause) {
    // "no matches" and "your pattern is broken" look identical in a preview,
    // and the author concludes the phrase is not there.
    return {
      matches: [],
      pages: [],
      diagnostics: [
        {
          code: "E0103",
          severity: "error",
          message: `\`${options.find}\` is not a valid regular expression: ${(cause as Error).message}`,
          url: "https://kasecrab.github.io/liyasa/docs/errors/E0103",
        },
      ],
    };
  }

  const matches: Match[] = [];
  const planned: PlannedPage[] = [];

  for (const page of pages) {
    const replaced = replaceInPage(page, pattern, options.replaceWith, scopes, matches);
    if (replaced) planned.push(replaced);
  }

  return { matches, pages: planned, diagnostics: [] };
}

/** ED-12's apply: the plan's own text, with nothing left to recompute. */
export function applyPlan(plan: BulkPlan): { path: string; text: string }[] {
  return plan.pages.map((page) => ({ path: page.path, text: page.after }));
}

/** A range of one segment's text a replace may touch, as offsets into it. */
interface Replaceable {
  start: number;
  end: number;
}

/**
 * The ranges of `text` — one segment's own text — a replace may rewrite.
 *
 * Offsets are relative to the segment, so every segment is rewritten on its
 * own and the page is the concatenation. Nothing has to know how much the
 * segments before it grew, which is where a preview and its edits drift apart.
 */
function replaceable(segment: SourceDocument["segments"][number], text: string, scopes: Set<Scope>): Replaceable[] {
  if (segment.segment === "markdown") {
    return scopes.has("text") ? [{ start: 0, end: text.length }] : [];
  }
  if (segment.segment === "code") {
    if (!scopes.has("code")) return [];
    const from = segment.body.start - segment.span.start;
    const to = segment.body.end - segment.span.start;
    // The spans are byte offsets and `text` is a string; they agree only when
    // the fence's opening line is ASCII, which an info string always is.
    return [{ start: byteToIndex(text, from), end: byteToIndex(text, to) }];
  }
  if (segment.segment === "directiveOpen" || segment.segment === "directiveLeaf") {
    if (!scopes.has("props")) return [];
    // Only the quoted values inside the attribute braces: rewriting a prop
    // *name* renames a prop the component does not have.
    const open = text.indexOf("{");
    const close = text.lastIndexOf("}");
    if (open === -1 || close <= open) return [];
    const ranges: Replaceable[] = [];
    for (const value of text.slice(open, close).matchAll(/"([^"]*)"|'([^']*)'/g)) {
      const at = (value.index ?? 0) + open + 1;
      ranges.push({ start: at, end: at + (value[1] ?? value[2] ?? "").length });
    }
    return ranges;
  }
  // A `template` segment is never replaceable, whatever the scopes say, and a
  // `directiveClose` has nothing in it but colons.
  return [];
}

function replaceInPage(
  page: ScannedPage,
  pattern: RegExp,
  replaceWith: string,
  scopes: Set<Scope>,
  matches: Match[],
): PlannedPage | null {
  const index = byteIndex(page.source);
  const starts = lineStarts(page.source);
  const found: Match[] = [];
  const edits: SegmentEdit[] = [];
  let after = page.document.frontmatter
    ? page.source.slice(index(page.document.frontmatter.span.start), index(page.document.frontmatter.span.end))
    : "";

  page.document.segments.forEach((segment, at) => {
    const from = index(segment.span.start);
    const text = page.source.slice(from, index(segment.span.end));
    const ranges = replaceable(segment, text, scopes);

    let rewritten = "";
    let copied = 0;
    let touched = false;
    for (const range of ranges) {
      const within = text.slice(range.start, range.end);
      pattern.lastIndex = 0;
      let replaced = "";
      let consumed = 0;
      for (const match of within.matchAll(pattern)) {
        const hit = match.index ?? 0;
        replaced += within.slice(consumed, hit) + replaceWith;
        consumed = hit + match[0].length;
        const absolute = from + range.start + hit;
        const line = lineOf(starts, absolute);
        found.push({
          path: page.path,
          segment: at,
          line: line + 1,
          column: absolute - (starts[line] as number) + 1,
          text: lineText(page.source, starts, line),
        });
      }
      if (consumed === 0) continue;
      touched = true;
      replaced += within.slice(consumed);
      rewritten += text.slice(copied, range.start) + replaced;
      copied = range.end;
    }

    if (!touched) {
      after += text;
      return;
    }
    rewritten += text.slice(copied);
    after += rewritten;
    edits.push({ segment: at, new_text: rewritten });
  });

  if (found.length === 0) return null;
  matches.push(...found);
  return { path: page.path, before: page.source, after, edits };
}

/** A byte offset into `text`, as a string index. */
function byteToIndex(text: string, offset: number): number {
  const encoder = new TextEncoder();
  let bytes = 0;
  let index = 0;
  for (const character of text) {
    if (bytes >= offset) break;
    bytes += encoder.encode(character).length;
    index += character.length;
  }
  return index;
}

export function changeFactReference(
  pages: ScannedPage[],
  options: { from: string; to: string },
): BulkPlan {
  // A fact id in prose is prose. Only the call is a reference, so the pattern
  // is anchored to `fact("...")` and the scope is the template segment that
  // holds it — the one scope a plain replace never touches.
  const pattern = new RegExp(`(fact\\(\\s*["'])${escapeRegExp(options.from)}(["']\\s*\\))`, "g");
  const matches: Match[] = [];
  const planned: PlannedPage[] = [];

  for (const page of pages) {
    const starts = lineStarts(page.source);
    const index = byteIndex(page.source);
    let after = "";
    let copied = 0;
    const edits: SegmentEdit[] = [];

    page.document.segments.forEach((segment, at) => {
      if (segment.segment !== "template" || segment.kind.kind === "comment") return;
      const start = index(segment.span.start);
      const end = index(segment.span.end);
      const text = page.source.slice(start, end);
      if (!pattern.test(text)) return;
      pattern.lastIndex = 0;
      const replaced = text.replace(pattern, `$1${options.to}$2`);
      const line = lineOf(starts, start);
      matches.push({
        path: page.path,
        segment: at,
        line: line + 1,
        column: start - (starts[line] as number) + 1,
        text: lineText(page.source, starts, line),
      });
      after += page.source.slice(copied, start) + replaced;
      copied = end;
      edits.push({ segment: at, new_text: replaced });
    });

    if (edits.length === 0) continue;
    after += page.source.slice(copied);
    planned.push({ path: page.path, before: page.source, after, edits });
  }

  return { matches, pages: planned, diagnostics: [] };
}

/** ED-12's "move a group": the group's node moves, its pages come with it. */
export function moveGroup(project: Project, options: { group: string; toIndex: number }): ProjectChange {
  const tree = navigationOf(project.config);
  const from = tree.findIndex((node) => node.group === options.group);
  if (from === -1) {
    return {
      writes: [],
      moves: [],
      deletes: [],
      redirects: [],
      config: project.config,
      diagnostics: [
        {
          code: "E0104",
          severity: "error",
          message: `there is no navigation group called \`${options.group}\`.`,
          url: "https://kasecrab.github.io/liyasa/docs/errors/E0104",
        },
      ],
    };
  }
  const next = [...tree];
  const [moved] = next.splice(from, 1);
  next.splice(Math.min(Math.max(options.toIndex, 0), next.length), 0, moved as NavigationGroup);
  const navigation = project.config["navigation"];
  const config = isRecord(navigation)
    ? { ...project.config, navigation: { ...navigation, [("tabs" in navigation ? "tabs" : "pages")]: next } }
    : { ...project.config, navigation: next };
  return { writes: [], moves: [], deletes: [], redirects: [], config, diagnostics: [] };
}

/** ED-12's "apply a tag": front matter only, and only where it changes. */
export function applyTag(
  pages: Record<string, string>,
  options: { paths: string[]; tag: string },
): { path: string; text: string }[] {
  const writes: { path: string; text: string }[] = [];
  for (const path of options.paths) {
    const text = pages[path];
    if (text === undefined) continue;
    if (parseFrontmatter(text).fields["tag"] === options.tag) continue;
    writes.push({ path, text: writeFrontmatter(text, { tag: options.tag }) });
  }
  return writes;
}

function escapeRegExp(text: string): string {
  return text.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

function lineStarts(text: string): number[] {
  const starts = [0];
  for (let at = 0; at < text.length; at += 1) if (text[at] === "\n") starts.push(at + 1);
  return starts;
}

function lineOf(starts: number[], at: number): number {
  let line = 0;
  while (line + 1 < starts.length && (starts[line + 1] as number) <= at) line += 1;
  return line;
}

function lineText(text: string, starts: number[], line: number): string {
  const start = starts[line] as number;
  const end = starts[line + 1] ?? text.length + 1;
  return text.slice(start, end - 1).replace(/\n$/, "");
}

function byteIndex(text: string): (offset: number) => number {
  const encoder = new TextEncoder();
  const map = new Map<number, number>();
  let at = 0;
  for (let index = 0; index < text.length; ) {
    map.set(at, index);
    const point = text.codePointAt(index) as number;
    const unit = String.fromCodePoint(point);
    at += encoder.encode(unit).length;
    index += unit.length;
  }
  map.set(at, text.length);
  const total = at;
  return (offset) => map.get(Math.min(Math.max(offset, 0), total)) ?? text.length;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
