// ED-02: the source mode.
//
// RFC 2430 records why this is not CodeMirror. What ED-02 asks CodeMirror for
// is three things — highlighting, completion, and a diagnostic gutter — and
// all three are decisions about the text rather than about a widget.
//
// The highlighting is derived from the same `Segment[]` the visual mode maps,
// deliberately. A source mode with its own tokenizer eventually disagrees with
// the visual mode about what a construct is, and the author sees whichever
// answer belongs to the pane they are in.
//
// One conversion happens here and is worth stating once: the API's spans are
// **byte** offsets into UTF-8, and the text a view indexes is a JavaScript
// string of UTF-16 units. Every offset this module returns is a string index,
// and `placeDiagnostics` reports columns in characters. A token placed at the
// byte offset covers the wrong text as soon as one non-ASCII character sits
// above it, and every test written in ASCII passes.

import type { Diagnostic, Segment, SourceDocument } from "../../../crates/liyasa-wasm/ts/liyasa-wasm.d.ts";

export type TokenKind =
  | "frontmatter"
  | "markdown"
  | "code"
  | "template-output"
  | "template-statement"
  | "template-comment"
  | "directive";

export interface Token {
  kind: TokenKind;
  /** String index, not a byte offset. */
  start: number;
  end: number;
}

export type DecorationKind = "heading" | "emphasis" | "strong" | "link" | "inline-code" | "autolink";

export interface Decoration {
  kind: DecorationKind;
  start: number;
  end: number;
}

export interface Position {
  line: number;
  column: number;
}

export interface PlacedDiagnostic {
  diagnostic: Diagnostic;
  from: Position;
  to: Position;
}

export interface CompletionContext {
  components?: string[];
  /** Prop names per component, from the component registry. */
  props?: Record<string, string[]>;
  facts?: string[];
  /** Variables the preview context holds. */
  variables?: string[];
}

export interface Completion {
  label: string;
  kind: "component" | "prop" | "function" | "statement" | "fact" | "variable";
  detail?: string;
}

/** Maps every byte offset in `text` to its string index. */
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

/** The whole file as highlightable runs, in order and with no gaps. */
export function tokenize(document: SourceDocument, text: string): Token[] {
  const index = byteIndex(text);
  const tokens: Token[] = [];
  if (document.frontmatter) {
    const span = document.frontmatter.span;
    tokens.push({ kind: "frontmatter", start: index(span.start), end: index(span.end) });
  }
  for (const segment of document.segments) {
    tokens.push({ kind: kindOf(segment), start: index(segment.span.start), end: index(segment.span.end) });
  }
  return tokens;
}

function kindOf(segment: Segment): TokenKind {
  switch (segment.segment) {
    case "markdown":
      return "markdown";
    case "code":
      return "code";
    case "template":
      if (segment.kind.kind === "output") return "template-output";
      if (segment.kind.kind === "statement") return "template-statement";
      return "template-comment";
    default:
      return "directive";
  }
}

const DECORATIONS: { kind: DecorationKind; pattern: RegExp }[] = [
  { kind: "heading", pattern: /^ {0,3}#{1,6} .*$/gm },
  { kind: "inline-code", pattern: /`[^`\n]+`/g },
  { kind: "link", pattern: /!?\[[^\]\n]*\]\([^)\n]*\)/g },
  { kind: "autolink", pattern: /<https?:\/\/[^>\s]+>/g },
  { kind: "strong", pattern: /\*\*[^*\n]+\*\*/g },
  { kind: "emphasis", pattern: /(?<!\*)\*[^*\n]+\*(?!\*)|_[^_\n]+_/g },
];

/**
 * The inline marks inside one markdown token.
 *
 * `offset` is where the token starts in the file, so the spans that come back
 * are absolute and the view does not have to add anything.
 */
export function decorate(text: string, offset: number): Decoration[] {
  const found: Decoration[] = [];
  for (const { kind, pattern } of DECORATIONS) {
    pattern.lastIndex = 0;
    for (const match of text.matchAll(pattern)) {
      const start = match.index ?? 0;
      found.push({ kind, start: offset + start, end: offset + start + match[0].length });
    }
  }
  // A `**bold**` inside a heading is both; sorting by start keeps the view's
  // painting order stable rather than regex-declaration order.
  return found.sort((left, right) => left.start - right.start || left.end - right.end);
}

/** The template statements minijinja accepts, as ED-02's autocomplete. */
const STATEMENTS = [
  "for",
  "endfor",
  "if",
  "elif",
  "else",
  "endif",
  "set",
  "include",
  "snippet",
  "import",
  "from",
  "macro",
  "endmacro",
  "with",
  "endwith",
  "filter",
  "endfilter",
  "raw",
  "endraw",
];

/** The functions and filters an output expression can call. */
const FUNCTIONS = ["fact(", "env(", "now(", "range(", "dict(", "url_for("];

/**
 * What to offer at `offset`.
 *
 * The construct the offset is inside decides the list. Offering component
 * names inside `{{ }}` and template functions after `:::` is an autocomplete
 * that is wrong more often than it is right, and an author learns to ignore it.
 */
export function completionsAt(text: string, offset: number, context: CompletionContext): Completion[] {
  const before = text.slice(0, offset);

  const directive = /(?:^|\n)(:{3,})([A-Za-z][\w-]*)?$/.exec(before);
  if (directive) {
    return prefixed(context.components ?? [], directive[2] ?? "").map((label) => ({ label, kind: "component" }));
  }

  const props = /(?:^|\n):{3,}([A-Za-z][\w-]*)\{([^}\n]*)$/.exec(before);
  if (props) {
    const component = props[1] as string;
    const typed = /([A-Za-z_][\w-]*)$/.exec(props[2] as string)?.[1] ?? "";
    return prefixed(context.props?.[component] ?? [], typed).map((label) => ({
      label,
      kind: "prop",
      detail: component,
    }));
  }

  const open = lastOpenTemplate(before);
  if (!open) return [];

  if (open.kind === "statement") {
    const typed = /([A-Za-z_]\w*)$/.exec(open.body)?.[1] ?? "";
    return prefixed(STATEMENTS, typed).map((label) => ({ label, kind: "statement" }));
  }

  // Inside `fact("...` the argument is a fact id, not an expression.
  const fact = /fact\(\s*["']([^"']*)$/.exec(open.body);
  if (fact) {
    return prefixed(context.facts ?? [], fact[1] as string).map((label) => ({ label, kind: "fact" }));
  }

  const typed = /([A-Za-z_][\w.]*)$/.exec(open.body)?.[1] ?? "";
  if (typed === "" && open.body.trim() !== "") return [];
  return [
    ...prefixed(FUNCTIONS, typed).map((label): Completion => ({ label, kind: "function" })),
    ...prefixed(context.variables ?? [], typed).map((label): Completion => ({ label, kind: "variable" })),
    ...prefixed(context.facts ?? [], typed).map((label): Completion => ({ label, kind: "fact" })),
  ];
}

/** The `{{` or `{%` the offset is inside, when it has not been closed. */
function lastOpenTemplate(before: string): { kind: "output" | "statement"; body: string } | null {
  const output = before.lastIndexOf("{{");
  const statement = before.lastIndexOf("{%");
  const at = Math.max(output, statement);
  if (at < 0) return null;
  const body = before.slice(at + 2);
  if (body.includes("}}") || body.includes("%}")) return null;
  return { kind: at === statement ? "statement" : "output", body };
}

function prefixed(candidates: string[], typed: string): string[] {
  if (typed === "") return [...candidates];
  return candidates.filter((candidate) => candidate.startsWith(typed));
}

/**
 * Where each diagnostic is shown.
 *
 * A diagnostic with no span belongs to the file rather than to a construct —
 * a front-matter or config finding — and is placed at the top. Dropping it is
 * how a source mode reports success on a page that does not build.
 */
export function placeDiagnostics(text: string, diagnostics: Diagnostic[]): PlacedDiagnostic[] {
  const index = byteIndex(text);
  const starts = lineStarts(text);
  const place = (offset: number): Position => {
    const at = index(offset);
    let line = 0;
    while (line + 1 < starts.length && (starts[line + 1] as number) <= at) line += 1;
    return { line: line + 1, column: at - (starts[line] as number) + 1 };
  };
  return diagnostics.map((diagnostic) => {
    if (!diagnostic.span) {
      return { diagnostic, from: { line: 1, column: 1 }, to: { line: 1, column: 1 } };
    }
    return { diagnostic, from: place(diagnostic.span.start), to: place(diagnostic.span.end) };
  });
}

function lineStarts(text: string): number[] {
  const starts = [0];
  for (let at = 0; at < text.length; at += 1) if (text[at] === "\n") starts.push(at + 1);
  return starts;
}
