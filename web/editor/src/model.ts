// ED-01's document model: the editor's tree is built from the **Source
// Document**, never from the Rendered AST.
//
// The distinction is the requirement. The Rendered AST is what a build
// produces after expansion, so a chip in it has already become its value and a
// loop has already become its rows; editing it and writing it back would write
// the expansion to disk. The Source Document is a lossless segmentation — its
// spans concatenate to the file's bytes — so a node here always knows which
// bytes it came from, which is what makes ED-03 possible at all.
//
// RFC 2430 records why this is not ProseMirror. The node set is the one ED-01
// names and the shape is a ProseMirror schema's; only the view is ours.

import type {
  PropValue,
  Segment,
  SegmentEdit,
  SourceDocument,
  Span,
} from "../../../crates/liyasa-wasm/ts/liyasa-wasm.d.ts";

/** The six node kinds ED-01 names. */
export type NodeKind = "markdown" | "chip" | "logic_block" | "component" | "code" | "opaque";

/** What a markdown segment's text is cut into for the visual mode. */
export type MarkdownBlockKind = "paragraph" | "heading" | "list" | "quote" | "table" | "html";

/**
 * One leaf of a markdown segment.
 *
 * `text` carries the block's bytes *including* the blank lines that follow it,
 * so concatenating a segment's blocks reproduces the segment. `start` and
 * `end` are offsets into the owning node's text, which is already decoded — the
 * byte arithmetic happens once, when the node is cut out of the source.
 */
export interface MarkdownBlock {
  id: string;
  kind: MarkdownBlockKind;
  text: string;
  start: number;
  end: number;
  level?: number;
}

export interface EditorNode {
  id: string;
  kind: NodeKind;
  /** The segment this node begins at; the index a `SegmentEdit` names. */
  segment: number;
  /** The node's source bytes, decoded. */
  text: string;
  /** The closing segment of a container, when it has one. */
  closes?: number;
  /** Blank lines before the first block of a markdown node. */
  lead?: string;
  blocks?: MarkdownBlock[];
  children?: EditorNode[];
  /** Directive name, or the statement keyword of a logic block. */
  name?: string;
  props?: Record<string, PropValue>;
  /** A logic block's statement, without its `{%` and `%}`. */
  expression?: string;
  lang?: string | null;
}

export interface EditorModel {
  /** The front matter's bytes, which `serialize_source` writes first. */
  frontmatter: string;
  nodes: EditorNode[];
  document: SourceDocument;
}

/** Anything the editor can address by id. */
export type Addressable = EditorNode | MarkdownBlock;

const ENCODER = new TextEncoder();
const DECODER = new TextDecoder();

function segmentSpan(segment: Segment): Span {
  return segment.span;
}

/** Builds the editor's tree from one parsed draft. */
export function buildModel(document: SourceDocument, source: string): EditorModel {
  const bytes = ENCODER.encode(source);
  const slice = (span: Span) => DECODER.decode(bytes.subarray(span.start, span.end));
  const frontmatter = document.frontmatter ? slice(document.frontmatter.span) : "";
  const nodes = buildRange(document, slice, 0, document.segments.length, "");
  return { frontmatter, nodes, document };
}

function buildRange(
  document: SourceDocument,
  slice: (span: Span) => string,
  from: number,
  to: number,
  prefix: string,
): EditorNode[] {
  const nodes: EditorNode[] = [];
  let at = from;
  while (at < to) {
    const segment = document.segments[at];
    if (!segment) break;
    const id = prefix === "" ? String(nodes.length) : `${prefix}/${nodes.length}`;
    const [node, next] = buildNode(document, slice, at, to, id);
    nodes.push(node);
    at = next;
  }
  return nodes;
}

function buildNode(
  document: SourceDocument,
  slice: (span: Span) => string,
  at: number,
  to: number,
  id: string,
): [EditorNode, number] {
  const segment = document.segments[at] as Segment;
  const span = segmentSpan(segment);

  if (segment.segment === "markdown") {
    return [markdownNode(id, at, slice(span)), at + 1];
  }

  if (segment.segment === "code") {
    return [
      { id, kind: "code", segment: at, text: slice(span), lang: segment.info.lang ?? null },
      at + 1,
    ];
  }

  if (segment.segment === "template") {
    if (segment.kind.kind === "output") {
      return [{ id, kind: "chip", segment: at, text: slice(span), expression: inner(slice(span)) }, at + 1];
    }
    if (segment.kind.kind === "statement") {
      const close = closingIndex(segment.kind.matching, at, to);
      if (close === null) return [opaque(id, at, slice(span)), at + 1];
      const closeSpan = segmentSpan(document.segments[close] as Segment);
      const text = sliceBetween(slice, span, closeSpan);
      // ED-05: WYSIWYG inside a loop body is deliberately not offered, because
      // a body is frequently partial Markdown such as a run of table rows. The
      // body is one opaque node and the view gives it a source mini-editor.
      const body = bodyChild(document, slice, at + 1, close, `${id}/0`);
      return [
        {
          id,
          kind: "logic_block",
          segment: at,
          closes: close,
          text,
          name: segment.kind.name,
          expression: inner(slice(span)),
          children: body ? [body] : [],
        },
        close + 1,
      ];
    }
    // A template comment is not modelled; it keeps its bytes.
    return [opaque(id, at, slice(span)), at + 1];
  }

  if (segment.segment === "directiveLeaf") {
    return [
      { id, kind: "component", segment: at, text: slice(span), name: segment.name, props: segment.props, children: [] },
      at + 1,
    ];
  }

  if (segment.segment === "directiveOpen") {
    const close = closingIndex(segment.matching, at, to);
    // ED-03(c): an unclosed container is a construct the visual editor does
    // not model. It carries its own bytes and nothing else — swallowing the
    // rest of the page would be the editor inventing a structure the file
    // does not have.
    if (close === null) return [opaque(id, at, slice(span)), at + 1];
    const closeSpan = segmentSpan(document.segments[close] as Segment);
    return [
      {
        id,
        kind: "component",
        segment: at,
        closes: close,
        text: sliceBetween(slice, span, closeSpan),
        name: segment.name,
        props: segment.props,
        children: buildRange(document, slice, at + 1, close, id),
      },
      close + 1,
    ];
  }

  // A close with no open: bytes, nothing more.
  return [opaque(id, at, slice(span)), at + 1];
}

/** A container's body collapsed into one node, or nothing when it is empty. */
function bodyChild(
  document: SourceDocument,
  slice: (span: Span) => string,
  from: number,
  to: number,
  id: string,
): EditorNode | null {
  if (from >= to) return null;
  const first = segmentSpan(document.segments[from] as Segment);
  const last = segmentSpan(document.segments[to - 1] as Segment);
  return { id, kind: "opaque", segment: from, closes: to - 1, text: sliceBetween(slice, first, last) };
}

function closingIndex(matching: number | null | undefined, at: number, to: number): number | null {
  if (matching === null || matching === undefined) return null;
  // A container whose close is outside the range being built is not closed as
  // far as this range is concerned, and treating it as closed would read
  // segments that belong to an enclosing node.
  if (matching <= at || matching >= to) return null;
  return matching;
}

function sliceBetween(slice: (span: Span) => string, from: Span, to: Span): string {
  return slice({ source: from.source, start: from.start, end: to.end });
}

function opaque(id: string, segment: number, text: string): EditorNode {
  return { id, kind: "opaque", segment, text };
}

/** `{{ name }}` and `{% for x in y %}` both give up their middle. */
function inner(text: string): string {
  return text.replace(/^\{[{%]-?/, "").replace(/-?[%}]\}$/, "").trim();
}

function markdownNode(id: string, segment: number, text: string): EditorNode {
  const { lead, blocks } = cutBlocks(id, text);
  return { id, kind: "markdown", segment, text, lead, blocks };
}

/**
 * Cuts a markdown segment into blocks on blank lines.
 *
 * A run of blank lines belongs to the block before it, so that concatenating
 * the blocks reproduces the segment. A run with nothing before it is the
 * node's `lead` rather than a block, because a block the author cannot see is
 * one they cannot edit.
 *
 * This is a presentational cut, not a CommonMark parse: a list with a blank
 * line between its items reads here as two blocks. Round-tripping does not
 * depend on it — that is byte arithmetic — and the classification only decides
 * which editing affordance a block gets.
 */
function cutBlocks(id: string, text: string): { lead: string; blocks: MarkdownBlock[] } {
  const lines = text.split(/(?<=\n)/);
  const blocks: MarkdownBlock[] = [];
  let lead = "";
  let at = 0;
  let index = 0;
  let current = "";
  let start = 0;
  let sawContent = false;

  const flush = () => {
    if (current === "") return;
    blocks.push({ ...classify(current), id: `${id}.${index}`, text: current, start, end: start + current.length });
    index += 1;
    current = "";
    sawContent = false;
  };

  for (const line of lines) {
    const blank = line.trim() === "";
    if (blank && !sawContent && current === "") {
      lead += line;
      at += line.length;
      continue;
    }
    if (blank) {
      current += line;
      at += line.length;
      continue;
    }
    // A non-blank line after a blank run closes the previous block.
    if (current !== "" && current.slice(-1) === "\n" && /\n\s*\n$/.test(current)) flush();
    if (current === "") start = at;
    current += line;
    at += line.length;
    sawContent = true;
  }
  flush();
  return { lead, blocks };
}

function classify(text: string): { kind: MarkdownBlockKind; level?: number } {
  const first = text.split("\n", 1)[0] ?? "";
  const body = first.replace(/^ {0,3}/, "");
  const heading = /^(#{1,6})\s/.exec(body);
  if (heading) return { kind: "heading", level: heading[1]?.length ?? 1 };
  if (body.startsWith(">")) return { kind: "quote" };
  if (/^([-*+]\s|\d+[.)]\s)/.test(body)) return { kind: "list" };
  if (body.startsWith("|")) return { kind: "table" };
  if (/^<[A-Za-z/!?]/.test(body)) return { kind: "html" };
  return { kind: "paragraph" };
}

/** Every node and block, depth first, in document order. */
export function flatten(model: EditorModel): Addressable[] {
  const out: Addressable[] = [];
  const walk = (nodes: EditorNode[]) => {
    for (const node of nodes) {
      out.push(node);
      for (const block of node.blocks ?? []) out.push(block);
      if (node.children) walk(node.children);
    }
  };
  walk(model.nodes);
  return out;
}

/** Resolves a node or block id. */
export function blockAt(model: EditorModel, id: string): Addressable | undefined {
  return flatten(model).find((item) => item.id === id);
}

/**
 * ED-03(a): the model written back out.
 *
 * Front matter first, then every top-level node's bytes — the same order
 * `liyasa_markdown::source::serialize::serialize_source` writes, because the
 * two have to agree about what an untouched document is.
 */
export function serializeModel(model: EditorModel): string {
  return model.frontmatter + model.nodes.map((node) => node.text).join("");
}

/**
 * ED-03(b): the edits that replace one block's text.
 *
 * The result names exactly one segment — the one the block lives in — and its
 * `new_text` differs from the segment's current bytes only inside the block.
 * Every other segment is absent from the list, and `serialize_source` copies an
 * unnamed segment straight from the source.
 */
export function editBlock(model: EditorModel, id: string, newText: string): SegmentEdit[] {
  const owner = ownerOf(model, id);
  if (!owner) throw new Error(`no block \`${id}\` in this document`);
  const { node, block } = owner;

  if (!block) {
    if (node.closes !== undefined && node.closes !== node.segment) {
      throw new Error(`\`${id}\` spans segments ${node.segment}..${node.closes}; edit its children or its props`);
    }
    if (newText === node.text) return [];
    return [{ segment: node.segment, new_text: newText }];
  }

  if (newText === block.text) return [];
  const replaced = node.text.slice(0, block.start) + newText + node.text.slice(block.end);
  return [{ segment: node.segment, new_text: replaced }];
}

function ownerOf(
  model: EditorModel,
  id: string,
): { node: EditorNode; block?: MarkdownBlock } | undefined {
  const walk = (nodes: EditorNode[]): { node: EditorNode; block?: MarkdownBlock } | undefined => {
    for (const node of nodes) {
      if (node.id === id) return { node };
      for (const block of node.blocks ?? []) if (block.id === id) return { node, block };
      const found = node.children ? walk(node.children) : undefined;
      if (found) return found;
    }
    return undefined;
  };
  return walk(model.nodes);
}
