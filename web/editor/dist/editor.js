(function () {
"use strict";

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
class Fragment {
  // A plain field and an explicit assignment: a TypeScript parameter property
  // is not erasable, and both `build.mjs` and `node --test` strip rather than
  // compile.
  value        ;

  constructor(value        ) {
    this.value = value;
  }

  toString()         {
    return this.value;
  }
}

/** For text and attribute values alike; `html` uses it on both. */
function escapeHtml(value         )         {
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
function html(strings                      , ...values           )           {
  let out = strings[0] ?? "";
  for (let i = 0; i < values.length; i += 1) {
    out += renderFragmentValue(values[i]) + (strings[i + 1] ?? "");
  }
  return new Fragment(out);
}

/** Marks a string as markup, for a fragment assembled by concatenation. */
function raw(value        )           {
  return new Fragment(value);
}

function renderFragmentValue(value         )         {
  if (value === null || value === undefined) return "";
  if (value instanceof Fragment) return value.value;
  if (Array.isArray(value)) return value.map(renderFragmentValue).join("");
  return escapeHtml(value);
}

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

             
            
          
              
                 
       
                                                         

/** The six node kinds ED-01 names. */
                                                                                             

/** What a markdown segment's text is cut into for the visual mode. */
                                                                                              

/**
 * One leaf of a markdown segment.
 *
 * `text` carries the block's bytes *including* the blank lines that follow it,
 * so concatenating a segment's blocks reproduces the segment. `start` and
 * `end` are offsets into the owning node's text, which is already decoded — the
 * byte arithmetic happens once, when the node is cut out of the source.
 */
                                
             
                          
               
                
              
                 
 

                             
             
                 
                                                                          
                  
                                          
               
                                                             
                  
                                                               
                
                           
                          
                                                                   
                
                                    
                                                              
                      
                       
 

                              
                                                                         
                      
                      
                           
 

/** Anything the editor can address by id. */
                                                     

const ENCODER = new TextEncoder();
const DECODER = new TextDecoder();

function segmentSpan(segment         )       {
  return segment.span;
}

/** Builds the editor's tree from one parsed draft. */
function buildModel(document                , source        )              {
  const bytes = ENCODER.encode(source);
  const slice = (span      ) => DECODER.decode(bytes.subarray(span.start, span.end));
  const frontmatter = document.frontmatter ? slice(document.frontmatter.span) : "";
  const nodes = buildRange(document, slice, 0, document.segments.length, "");
  return { frontmatter, nodes, document };
}

function buildRange(
  document                ,
  slice                        ,
  from        ,
  to        ,
  prefix        ,
)               {
  const nodes               = [];
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
  document                ,
  slice                        ,
  at        ,
  to        ,
  id        ,
)                       {
  const segment = document.segments[at]           ;
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
      const closeSpan = segmentSpan(document.segments[close]           );
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
    const closeSpan = segmentSpan(document.segments[close]           );
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
  document                ,
  slice                        ,
  from        ,
  to        ,
  id        ,
)                    {
  if (from >= to) return null;
  const first = segmentSpan(document.segments[from]           );
  const last = segmentSpan(document.segments[to - 1]           );
  return { id, kind: "opaque", segment: from, closes: to - 1, text: sliceBetween(slice, first, last) };
}

function closingIndex(matching                           , at        , to        )                {
  if (matching === null || matching === undefined) return null;
  // A container whose close is outside the range being built is not closed as
  // far as this range is concerned, and treating it as closed would read
  // segments that belong to an enclosing node.
  if (matching <= at || matching >= to) return null;
  return matching;
}

function sliceBetween(slice                        , from      , to      )         {
  return slice({ source: from.source, start: from.start, end: to.end });
}

function opaque(id        , segment        , text        )             {
  return { id, kind: "opaque", segment, text };
}

/** `{{ name }}` and `{% for x in y %}` both give up their middle. */
function inner(text        )         {
  return text.replace(/^\{[{%]-?/, "").replace(/-?[%}]\}$/, "").trim();
}

function markdownNode(id        , segment        , text        )             {
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
function cutBlocks(id        , text        )                                            {
  const lines = text.split(/(?<=\n)/);
  const blocks                  = [];
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

function classify(text        )                                              {
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
function flatten(model             )                {
  const out                = [];
  const walk = (nodes              ) => {
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
function blockAt(model             , id        )                          {
  return flatten(model).find((item) => item.id === id);
}

/**
 * ED-03(a): the model written back out.
 *
 * Front matter first, then every top-level node's bytes — the same order
 * `liyasa_markdown::source::serialize::serialize_source` writes, because the
 * two have to agree about what an untouched document is.
 */
function serializeModel(model             )         {
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
function editBlock(model             , id        , newText        )                {
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

/**
 * Several block edits at once, as one `SegmentEdit` per segment touched.
 *
 * Applying `editBlock` repeatedly would not do: each call computes its
 * replacement from the segment's *original* text, so two edits in one segment
 * produce two edits that each undo the other. The offsets are applied from the
 * end backwards, so an earlier block's offsets are still valid when it is
 * reached.
 */
function editBlocks(
  model             ,
  changes                                ,
)                {
  const bySegment = new Map                                                                                ();

  for (const change of changes) {
    const owner = ownerOf(model, change.id);
    if (!owner) throw new Error(`no block \`${change.id}\` in this document`);
    const { node, block } = owner;
    if (!block) {
      if (node.closes !== undefined && node.closes !== node.segment) {
        throw new Error(`\`${change.id}\` spans segments ${node.segment}..${node.closes}`);
      }
      bySegment.set(node.segment, { node, blocks: [] });
      continue;
    }
    const entry = bySegment.get(node.segment) ?? { node, blocks: [] };
    entry.blocks.push({ block, text: change.text });
    bySegment.set(node.segment, entry);
  }

  const edits                = [];
  for (const [segment, entry] of [...bySegment.entries()].sort((left, right) => left[0] - right[0])) {
    if (entry.blocks.length === 0) {
      const only = changes.find((change) => change.id === entry.node.id);
      if (only && only.text !== entry.node.text) edits.push({ segment, new_text: only.text });
      continue;
    }
    let text = entry.node.text;
    for (const { block, text: replacement } of [...entry.blocks].sort((left, right) => right.block.start - left.block.start)) {
      text = text.slice(0, block.start) + replacement + text.slice(block.end);
    }
    if (text !== entry.node.text) edits.push({ segment, new_text: text });
  }
  return edits;
}

function ownerOf(
  model             ,
  id        ,
)                                                          {
  const walk = (nodes              )                                                          => {
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

// The text arithmetic several modules need, in one place.
//
// It is here rather than repeated because the bundler requires every binding
// to be declared once across the entry graph (RFC 1100), and because the byte
// conversion in particular is the kind of thing that is right in one copy and
// subtly wrong in the next: the API's spans are **byte** offsets into UTF-8 and
// every string the editor holds is UTF-16.

/** Maps a byte offset in `text` to its string index. Builds the map once. */
function byteIndex(text        )                             {
  const encoder = new TextEncoder();
  const map = new Map                ();
  let at = 0;
  for (let index = 0; index < text.length; ) {
    map.set(at, index);
    const point = text.codePointAt(index)          ;
    const unit = String.fromCodePoint(point);
    at += encoder.encode(unit).length;
    index += unit.length;
  }
  map.set(at, text.length);
  const total = at;
  return (offset) => map.get(Math.min(Math.max(offset, 0), total)) ?? text.length;
}

/** One byte offset, without building a map. For a short string or a single call. */
function byteToIndex(text        , offset        )         {
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

/** The string index each line of `text` starts at, the first being 0. */
function lineStarts(text        )           {
  const starts = [0];
  for (let at = 0; at < text.length; at += 1) if (text[at] === "\n") starts.push(at + 1);
  return starts;
}

/** The 0-based line an offset falls on. */
function lineOf(starts          , at        )         {
  let line = 0;
  while (line + 1 < starts.length && (starts[line + 1]          ) <= at) line += 1;
  return line;
}

/** One line of `text`, without its newline. */
function lineText(text        , starts          , line        )         {
  const start = starts[line]          ;
  const end = starts[line + 1] ?? text.length + 1;
  return text.slice(start, end - 1).replace(/\n$/, "");
}

/** Escapes `text` so it matches itself inside a regular expression. */
function escapeRegExp(text        )         {
  return text.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

/** A plain object, and not an array or `null`. */
function isRecord(value         )                                   {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

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

/** The whole file as highlightable runs, in order and with no gaps. */
function tokenize(document                , text        )          {
  const index = byteIndex(text);
  const tokens          = [];
  if (document.frontmatter) {
    const span = document.frontmatter.span;
    tokens.push({ kind: "frontmatter", start: index(span.start), end: index(span.end) });
  }
  for (const segment of document.segments) {
    tokens.push({ kind: kindOf(segment), start: index(segment.span.start), end: index(segment.span.end) });
  }
  return tokens;
}

function kindOf(segment         )            {
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

const DECORATIONS                                              = [
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
function decorate(text        , offset        )               {
  const found               = [];
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
function completionsAt(text        , offset        , context                   )               {
  const before = text.slice(0, offset);

  const directive = /(?:^|\n)(:{3,})([A-Za-z][\w-]*)?$/.exec(before);
  if (directive) {
    return prefixed(context.components ?? [], directive[2] ?? "").map((label) => ({ label, kind: "component" }));
  }

  const props = /(?:^|\n):{3,}([A-Za-z][\w-]*)\{([^}\n]*)$/.exec(before);
  if (props) {
    const component = props[1]          ;
    const typed = /([A-Za-z_][\w-]*)$/.exec(props[2]          )?.[1] ?? "";
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
    return prefixed(context.facts ?? [], fact[1]          ).map((label) => ({ label, kind: "fact" }));
  }

  const typed = /([A-Za-z_][\w.]*)$/.exec(open.body)?.[1] ?? "";
  if (typed === "" && open.body.trim() !== "") return [];
  return [
    ...prefixed(FUNCTIONS, typed).map((label)             => ({ label, kind: "function" })),
    ...prefixed(context.variables ?? [], typed).map((label)             => ({ label, kind: "variable" })),
    ...prefixed(context.facts ?? [], typed).map((label)             => ({ label, kind: "fact" })),
  ];
}

/** The `{{` or `{%` the offset is inside, when it has not been closed. */
function lastOpenTemplate(before        )                                                        {
  const output = before.lastIndexOf("{{");
  const statement = before.lastIndexOf("{%");
  const at = Math.max(output, statement);
  if (at < 0) return null;
  const body = before.slice(at + 2);
  if (body.includes("}}") || body.includes("%}")) return null;
  return { kind: at === statement ? "statement" : "output", body };
}

function prefixed(candidates          , typed        )           {
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
function placeDiagnostics(text        , diagnostics              )                     {
  const index = byteIndex(text);
  const starts = lineStarts(text);
  const place = (offset        )           => {
    const at = index(offset);
    let line = 0;
    while (line + 1 < starts.length && (starts[line + 1]          ) <= at) line += 1;
    return { line: line + 1, column: at - (starts[line]          ) + 1 };
  };
  return diagnostics.map((diagnostic) => {
    if (!diagnostic.span) {
      return { diagnostic, from: { line: 1, column: 1 }, to: { line: 1, column: 1 } };
    }
    return { diagnostic, from: place(diagnostic.span.start), to: place(diagnostic.span.end) };
  });
}

// ED-04: the three ways an author adds or moves a block without leaving the
// keyboard — a slash command, a Markdown shortcut, and a drag.
//
// All three produce `SegmentEdit`s against the model. Nothing here touches the
// document; `editor.ts` does, and that is what makes every rule below
// testable without a browser.

                                                                                   
                                                                         

                               
               
                             
                
                                                        
                    
                 
 

/**
 * The seven ED-04 names.
 *
 * Each insertion is Markdown the build already parses — a container nests with
 * more colons than its children, the way every directive under `docs/` is
 * written, and a snippet uses CM-70's `{% snippet %}` rather than the include
 * it desugars to. An insertion that needed a later pass to become valid would
 * be a page that does not build between the two.
 */
const SLASH_COMMANDS                 = [
  {
    name: "callout",
    label: "Callout",
    aliases: ["note", "warning", "tip", "info", "admonition"],
    insert: ':::note{title="Heads up"}\nSomething worth reading.\n:::\n',
  },
  {
    name: "tabs",
    label: "Tabs",
    aliases: ["tab", "switcher"],
    insert: '::::tabs\n\n:::tab{title="First"}\n\n:::\n\n:::tab{title="Second"}\n\n:::\n\n::::\n',
  },
  { name: "image", label: "Image", aliases: ["picture", "screenshot", "figure"], insert: "![](/assets/)\n" },
  { name: "code", label: "Code block", aliases: ["fence", "sample", "snippet of code"], insert: "```bash\n\n```\n" },
  { name: "table", label: "Table", aliases: ["grid", "rows"], insert: "| Column | Column |\n| --- | --- |\n|  |  |\n" },
  { name: "fact", label: "Fact", aliases: ["number", "value", "price"], insert: '{{ fact("") }}\n' },
  { name: "snippet", label: "Snippet", aliases: ["include", "reuse", "partial"], insert: '{% snippet "" %}\n' },
];

/** The Markdown a command inserts. */
function slashInsert(name        )         {
  const command = SLASH_COMMANDS.find((candidate) => candidate.name === name);
  if (!command) throw new Error(`no slash command \`${name}\``);
  return command.insert;
}

/** The menu's filter: name, label and alias, so a writer can type the intent. */
function slashMatches(query        )                 {
  const needle = query.trim().toLowerCase();
  if (needle === "") return [...SLASH_COMMANDS];
  return SLASH_COMMANDS.filter((command) =>
    [command.name, command.label, ...command.aliases].some((term) => term.toLowerCase().includes(needle)),
  );
}

                                                                                       

                           
                  
                     
                 
 

const MARKDOWN_SHORTCUTS                                            = [
  { pattern: /^(#{1,6} )$/, kind: "heading" },
  { pattern: /^([-*+] )$/, kind: "list" },
  { pattern: /^(\d+[.)] )$/, kind: "ordered" },
  { pattern: /^(> )$/, kind: "quote" },
  { pattern: /^(```|~~~)$/, kind: "fence" },
  { pattern: /^(---|\*\*\*)$/, kind: "rule" },
];

/**
 * What the text typed so far at the start of a block turns into.
 *
 * `null` for anything else, including a marker in the middle of a line: a
 * shortcut that fires mid-sentence rewrites prose the author was writing.
 */
function matchShortcut(typed        )                  {
  for (const { pattern, kind } of MARKDOWN_SHORTCUTS) {
    const found = pattern.exec(typed);
    if (!found) continue;
    const replace = found[1]          ;
    if (kind === "heading") return { replace, kind, level: replace.trimEnd().length };
    return { replace, kind };
  }
  return null;
}

/**
 * ED-04's drag reorder, as an edit to one segment.
 *
 * A block's text carries the blank lines that follow it, so moving the text
 * verbatim would move the separators with it and leave the run at the end of
 * the document attached to the wrong block. The separator belongs to the
 * position, not to the block: bodies move, gaps stay.
 */
function moveBlock(model             , id        , to        )                {
  const owner = ownerOfBlock(model, id);
  if (!owner) throw new Error(`no block \`${id}\` in this document`);
  const { node, blocks } = owner;
  const from = blocks.findIndex((block) => block.id === id);
  if (to < 0 || to >= blocks.length) {
    throw new Error(`position ${to} is outside this segment's ${blocks.length} blocks`);
  }
  if (to === from) return [];

  const split = blocks.map(splitGap);
  const bodies = split.map((part) => part.body);
  const gaps = split.map((part) => part.gap);
  const [moved] = bodies.splice(from, 1);
  bodies.splice(to, 0, moved          );

  const rebuilt = (node.lead ?? "") + bodies.map((body, at) => body + (gaps[at] ?? "")).join("");
  if (rebuilt === node.text) return [];
  return [{ segment: node.segment, new_text: rebuilt }];
}

/** A block's content, and the blank lines that separate it from the next. */
function splitGap(block               )                                {
  const lines = block.text.split(/(?<=\n)/);
  let at = lines.length;
  while (at > 0 && (lines[at - 1] ?? "").trim() === "") at -= 1;
  return { body: lines.slice(0, at).join(""), gap: lines.slice(at).join("") };
}

function ownerOfBlock(
  model             ,
  id        ,
)                                                            {
  const walk = (nodes              )                                                            => {
    for (const node of nodes) {
      const blocks = node.blocks ?? [];
      if (blocks.some((block) => block.id === id)) return { node, blocks };
      const found = node.children ? walk(node.children) : undefined;
      if (found) return found;
    }
    return undefined;
  };
  return walk(model.nodes);
}

// ED-04's paste: clipboard HTML from Google Docs, Notion, Confluence or a
// plain web page, converted to Liyasa Markdown.
//
// The HTML is parsed here rather than with `DOMParser` so that the browser and
// `node --test` run the same code. A converter whose tests drive a stand-in
// DOM is a converter nobody has tested.
//
// Two rules the conversions share:
//
//   * A paste is untrusted text. `script`, `style` and every event attribute
//     are dropped before anything is read, and text nodes are escaped, so a
//     paste cannot introduce Markdown or HTML the author did not type.
//   * An image becomes a media-library asset, not a remote URL. A pasted image
//     that stays remote is a link that breaks when the source does, and CM-131
//     never sees the bytes it is supposed to strip.

/** Where a paste came from, as far as its markup gives it away. */
                                                                           

                              
              
              
                                                                           
               
 

                   
              
                                
                       
 

                                           

const DROPPED = new Set(["script", "style", "meta", "head", "title", "link", "noscript", "svg"]);
const VOID = new Set(["br", "hr", "img", "input", "meta", "link", "col", "source", "wbr", "area", "base"]);

/**
 * Confluence's storage format wraps a callout in a macro element. Mapping it
 * to the matching Liyasa directive is the difference between "clean
 * conversion" and a paste that loses the box it was in.
 */
const CONFLUENCE_MACROS                         = {
  info: "info",
  note: "note",
  tip: "tip",
  warning: "warning",
  panel: "note",
};

function pasteSource(html        )              {
  if (/id="docs-internal-guid|docs\.google\.com/.test(html)) return "google-docs";
  if (/notion-|data-pm-slice|www\.notion\.so/.test(html)) return "notion";
  if (/confluenceT[dh]|ac:structured-macro|atlassian/.test(html)) return "confluence";
  return "html";
}

/** Every image a paste carries, in document order. */
function imagesIn(html        )                {
  const found                = [];
  walk(parseHtml(html), (node) => {
    if (node.tag !== "img") return;
    const src = node.attrs["src"] ?? "";
    if (src === "") return;
    found.push({ src, alt: node.attrs["alt"] ?? "", path: assetPath(src) });
  });
  return found;
}

/** The whole conversion: clipboard HTML in, Markdown out. */
function htmlToMarkdown(html        )         {
  const blocks = renderBlocks(parseHtml(html));
  const text = blocks.filter((block) => block !== "").join("\n\n");
  return text === "" ? "" : `${text}\n`;
}

/** `https://host/path/diagram.png?v=2` becomes `/assets/diagram.png`. */
function assetPath(src        )         {
  const withoutQuery = src.split(/[?#]/, 1)[0] ?? src;
  const name = withoutQuery.split("/").filter((part) => part !== "").pop() ?? "image";
  return `/assets/${name}`;
}

// --- the parser -------------------------------------------------------------

const TOKEN = /<!--[\s\S]*?-->|<\/?[A-Za-z][^>]*>|[^<]+/g;
const ATTR = /([A-Za-z_:][-\w:.]*)(?:\s*=\s*(?:"([^"]*)"|'([^']*)'|([^\s"'>]+)))?/g;

function parseHtml(html        )          {
  const root          = { tag: "#root", attrs: {}, children: [] };
  const stack            = [root];
  let skipping                = null;

  for (const raw of html.match(TOKEN) ?? []) {
    if (raw.startsWith("<!--")) continue;

    if (skipping !== null) {
      if (raw.toLowerCase() === `</${skipping}>`) skipping = null;
      continue;
    }

    if (!raw.startsWith("<")) {
      const top = stack[stack.length - 1];
      if (top) top.children.push({ text: decodeEntities(raw) });
      continue;
    }

    if (raw.startsWith("</")) {
      const tag = tagName(raw.slice(2));
      // An unbalanced close is common in clipboard HTML; unwind to the
      // matching open when there is one and ignore it when there is not.
      const at = lastIndexOf(stack, tag);
      if (at > 0) stack.length = at;
      continue;
    }

    const tag = tagName(raw.slice(1));
    if (DROPPED.has(tag)) {
      if (!raw.endsWith("/>") && !VOID.has(tag)) skipping = tag;
      continue;
    }
    const element          = { tag, attrs: attributes(raw), children: [] };
    const top = stack[stack.length - 1];
    if (top) top.children.push(element);
    if (!VOID.has(tag) && !raw.endsWith("/>")) stack.push(element);
  }
  return root;
}

function tagName(rest        )         {
  const name = /^[A-Za-z][-\w:]*/.exec(rest)?.[0] ?? "";
  return name.toLowerCase();
}

function lastIndexOf(stack           , tag        )         {
  for (let i = stack.length - 1; i > 0; i -= 1) if (stack[i]?.tag === tag) return i;
  return -1;
}

function attributes(raw        )                         {
  const attrs                         = {};
  const rest = raw.replace(/^<[A-Za-z][-\w:]*/, "").replace(/\/?>$/, "");
  for (const match of rest.matchAll(ATTR)) {
    const name = (match[1] ?? "").toLowerCase();
    // `onclick`, `onerror` and their kind never reach the document.
    if (name.startsWith("on")) continue;
    attrs[name] = decodeEntities(match[2] ?? match[3] ?? match[4] ?? "");
  }
  return attrs;
}

const ENTITIES                         = {
  amp: "&",
  lt: "<",
  gt: ">",
  quot: '"',
  apos: "'",
  nbsp: " ",
};

function decodeEntities(text        )         {
  return text.replace(/&(#x?[0-9A-Fa-f]+|[A-Za-z]+);/g, (whole, body        ) => {
    if (body.startsWith("#x") || body.startsWith("#X")) {
      return String.fromCodePoint(Number.parseInt(body.slice(2), 16));
    }
    if (body.startsWith("#")) return String.fromCodePoint(Number.parseInt(body.slice(1), 10));
    return ENTITIES[body.toLowerCase()] ?? whole;
  });
}

function walk(node          , visit                            )       {
  if (!("tag" in node)) return;
  visit(node);
  for (const child of node.children) walk(child, visit);
}

// --- blocks -----------------------------------------------------------------

const HEADINGS                         = { h1: "#", h2: "##", h3: "###", h4: "####", h5: "#####", h6: "######" };

function renderBlocks(node         )           {
  const out           = [];
  for (const child of node.children) {
    if (!("tag" in child)) {
      const loose = escapeText(collapse(child.text));
      if (loose.trim() !== "") out.push(lineStartSafe(loose.trim()));
      continue;
    }
    out.push(...blockFor(child));
  }
  return out;
}

function blockFor(element         )           {
  const { tag } = element;

  const heading = HEADINGS[tag];
  if (heading) {
    const text = inline(element).trim();
    return text === "" ? [] : [`${heading} ${text}`];
  }

  if (tag === "p") {
    const text = inline(element).trim();
    return text === "" ? [] : [lineStartSafe(text)];
  }

  if (tag === "ul" || tag === "ol") return [list(element, tag === "ol")];

  if (tag === "blockquote") {
    const inner = renderBlocks(element).filter((block) => block !== "");
    if (inner.length === 0) return [];
    return [inner.join("\n\n").split("\n").map((line) => (line === "" ? ">" : `> ${line}`)).join("\n")];
  }

  if (tag === "pre") return [fence(element)];

  if (tag === "table") return [table(element)];

  if (tag === "img") {
    const src = element.attrs["src"] ?? "";
    if (src === "") return [];
    return [`![${escapeText(element.attrs["alt"] ?? "")}](${assetPath(src)})`];
  }

  if (tag === "hr") return ["---"];

  // Confluence wraps a callout in `<ac:structured-macro ac:name="info">`.
  if (tag === "ac:structured-macro") {
    const name = CONFLUENCE_MACROS[element.attrs["ac:name"] ?? ""] ?? "note";
    const body = renderBlocks(element).filter((block) => block !== "");
    return [`:::${name}\n${body.join("\n\n")}\n:::`];
  }

  // Anything else is a container: its blocks are the document's blocks. A
  // container that holds only inline content is a paragraph, and it keeps its
  // own inline meaning — `inline(element)` would read `<b>text</b>` at block
  // position as plain text, losing the emphasis the author applied.
  if (element.children.some(isBlockLevel)) return renderBlocks(element);
  const text = inlineFor(element).trim();
  return text === "" ? [] : [lineStartSafe(text)];
}

const BLOCK_LEVEL = new Set([
  "address", "article", "aside", "blockquote", "div", "dl", "figure", "footer",
  "form", "h1", "h2", "h3", "h4", "h5", "h6", "header", "hr", "li", "main",
  "nav", "ol", "p", "pre", "section", "table", "ul", "ac:structured-macro",
  "ac:rich-text-body", "figcaption",
]);

function isBlockLevel(node          )          {
  if (!("tag" in node)) return false;
  if (BLOCK_LEVEL.has(node.tag)) return true;
  // Google Docs and Notion both wrap block content in an inline tag, so what
  // makes a container is having a block descendant, not being one.
  return node.children.some(isBlockLevel);
}

function list(element         , ordered         )         {
  const rows           = [];
  let index = 0;
  for (const child of element.children) {
    if (!("tag" in child) || child.tag !== "li") continue;
    index += 1;
    const marker = ordered ? `${index}.` : "-";
    const blocks = renderBlocks(child).filter((block) => block !== "");
    const body = blocks.length > 0 ? blocks.join("\n\n") : inline(child).trim();
    const indented = body.split("\n").map((line, at) => (at === 0 ? line : `  ${line}`)).join("\n");
    rows.push(`${marker} ${indented}`);
  }
  return rows.join("\n");
}

function fence(element         )         {
  const code = element.children.find((child) => "tag" in child && child.tag === "code");
  const inner = code && "tag" in code ? code : element;
  const language = /language-([\w-]+)/.exec(inner.attrs["class"] ?? "")?.[1] ?? "";
  const body = rawText(inner).replace(/\n+$/, "");
  return `\`\`\`${language}\n${body}\n\`\`\``;
}

function table(element         )         {
  const rows             = [];
  walk(element, (node) => {
    if (node.tag !== "tr") return;
    const cells           = [];
    for (const cell of node.children) {
      if (!("tag" in cell) || (cell.tag !== "td" && cell.tag !== "th")) continue;
      cells.push(inline(cell).trim().replace(/\|/g, "\\|"));
    }
    if (cells.length > 0) rows.push(cells);
  });
  if (rows.length === 0) return "";
  const header = rows[0]            ;
  const lines = [`| ${header.join(" | ")} |`, `| ${header.map(() => "---").join(" | ")} |`];
  for (const row of rows.slice(1)) lines.push(`| ${row.join(" | ")} |`);
  return lines.join("\n");
}

// --- inline -----------------------------------------------------------------

function inline(element         )         {
  let out = "";
  for (const child of element.children) {
    if (!("tag" in child)) {
      out += escapeText(collapse(child.text));
      continue;
    }
    out += inlineFor(child);
  }
  return out.replace(/[ \t]+/g, " ");
}

function inlineFor(element         )         {
  const { tag } = element;
  if (tag === "br") return "\n";
  if (tag === "img") {
    const src = element.attrs["src"] ?? "";
    return src === "" ? "" : `![${escapeText(element.attrs["alt"] ?? "")}](${assetPath(src)})`;
  }
  if (tag === "code") return `\`${rawText(element)}\``;
  if (tag === "a") {
    const href = element.attrs["href"] ?? "";
    const text = inline(element).trim();
    return href === "" ? text : `[${text}](${href})`;
  }
  if (tag === "strong" || (tag === "b" && !isNormalWeight(element))) return wrap(inline(element), "**");
  if (tag === "em" || tag === "i") return wrap(inline(element), "*");
  if (tag === "del" || tag === "s") return wrap(inline(element), "~~");
  if (tag === "span" && isBold(element)) return wrap(inline(element), "**");
  if (tag === "span" && isItalic(element)) return wrap(inline(element), "*");
  return inline(element);
}

/**
 * Google Docs wraps the whole clipboard in `<b style="font-weight:normal">`.
 * Reading that as bold marks the entire paste `**...**`, which is exactly what
 * a converter that looks only at tag names does. The weight in the style wins
 * over the tag, for `b` and for `span` alike.
 */
function isNormalWeight(element         )          {
  const weight = styleValue(element, "font-weight");
  return weight === "normal" || weight === "400";
}

function isBold(element         )          {
  const weight = styleValue(element, "font-weight");
  return weight === "bold" || (/^\d+$/.test(weight) && Number(weight) >= 600);
}

function isItalic(element         )          {
  return styleValue(element, "font-style") === "italic";
}

function styleValue(element         , property        )         {
  const style = element.attrs["style"] ?? "";
  const found = new RegExp(`(?:^|;)\\s*${property}\\s*:\\s*([^;]+)`, "i").exec(style);
  return (found?.[1] ?? "").trim().toLowerCase();
}

/** Keeps the surrounding spaces outside the markers, where Markdown needs them. */
function wrap(text        , marker        )         {
  const inner = text.trim();
  if (inner === "") return "";
  const before = /^\s/.test(text) ? " " : "";
  const after = /\s$/.test(text) ? " " : "";
  return `${before}${marker}${inner}${marker}${after}`;
}

function rawText(node          )         {
  if (!("tag" in node)) return node.text;
  return node.children.map(rawText).join("");
}

function collapse(text        )         {
  return text.replace(/\s+/g, " ");
}

const ESCAPED = /[\\`*_[\]<>]/g;

function escapeText(text        )         {
  return text.replace(ESCAPED, (character) => `\\${character}`);
}

/** A block whose first characters would otherwise open a different block. */
function lineStartSafe(text        )         {
  return text.replace(/^(#{1,6}(?=\s|$)|>|[-+](?=\s)|\d+[.)](?=\s))/, "\\$1");
}

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

/** ED-05's numbers: 50 rows and 256 KB. */
const PREVIEW_DEFAULTS                = { maxIterations: 50, maxBytes: 256 * 1024 };

                                 
                   
                  
                   
                  
                         
 

/** The layers the host already holds for the open draft. */
                             
                                      
                                  
                                 
                                 
                
                                
 

const UNITS                         = { B: 1, KB: 1024, MB: 1024 * 1024, GB: 1024 * 1024 * 1024 };
const SIZE = /^(\d+(?:\.\d+)?)\s*(B|KB|MB|GB)$/;

/**
 * The caps in force, and what was wrong with the configuration.
 *
 * A value the schema would reject falls back to the default *and* reports
 * `E0102`. Falling back quietly means the author configured a cap, the editor
 * used a different one, and nothing said so.
 */
function previewLimits(config         )                                                       {
  const diagnostics               = [];
  const preview = pathOf(config, ["editor", "preview"]);
  const limits                = { ...PREVIEW_DEFAULTS };
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

function parseSize(written        )                {
  const found = SIZE.exec(written.trim());
  if (!found) return null;
  const unit = UNITS[(found[2]          ).toUpperCase()];
  if (unit === undefined) return null;
  return Math.round(Number(found[1]) * unit);
}

function schemaError(key        , expected        , found         )             {
  return {
    code: "E0102",
    severity: "error",
    message: `\`${key}\` expects ${expected}, found ${JSON.stringify(found)}; the editor used its default`,
    url: "https://kasecrab.github.io/liyasa/docs/errors/E0102",
  };
}

                                
             
                
                     
 

/**
 * The rows a loop preview draws.
 *
 * `total` is the whole expansion, not the drawn part: "50 of 50" and "50 of
 * 900" are different statements, and the author needs the second to know the
 * preview is partial. It is also what the "show all" control counts.
 */
function capIterations   (rows     , limits               )                {
  if (rows.length <= limits.maxIterations) {
    return { shown: [...rows], total: rows.length, truncated: false };
  }
  return { shown: rows.slice(0, limits.maxIterations), total: rows.length, truncated: true };
}

                             
               
                
                     
 

/**
 * The preview text within the byte cap.
 *
 * The cut lands on a character boundary. Cutting a UTF-8 sequence in half puts
 * a replacement character on screen and makes the editor look like it
 * corrupted the page it is previewing.
 */
function capBytes(text        , limits               )             {
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
function previewContext(toolbar                , layers            )                          {
  const context                          = { ...(layers.variables ?? {}) };
  for (const dimension of ["version", "locale", "product", "region"]         ) {
    const chosen = toolbar[dimension];
    if (chosen !== undefined && chosen !== "") context[dimension] = chosen;
  }
  for (const [name, layer] of [
    ["facts", layers.facts],
    ["page", layers.page],
    ["site", layers.site],
    ["nav", layers.nav],
    ["env", layers.env],
  ]         ) {
    if (layer !== undefined && layer !== null) context[name] = layer;
  }
  if (toolbar.readerGroups.length > 0) context["reader"] = { groups: [...toolbar.readerGroups] };
  return context;
}

                             
                                                               
               
                 
 

/**
 * ED-05's tooltip: where a chip's value came from.
 *
 * The answer is read from the page's own `ExpansionRecord`, so the tooltip
 * cannot claim a source that the expansion did not record. An expression the
 * record does not mention says exactly that rather than guessing.
 */
function chipSource(expression        , record                 )             {
  const trimmed = expression.trim();

  const fact = /^fact\(\s*["']([^"']+)["']\s*\)$/.exec(trimmed);
  if (fact && record.facts.includes(fact[1]          )) {
    return { kind: "fact", name: fact[1]          , detail: "from facts/" };
  }

  const env = /^env\(\s*["']([^"']+)["']\s*\)$/.exec(trimmed);
  if (env && record.env.includes(env[1]          )) {
    return { kind: "env", name: env[1]          , detail: "from build.env" };
  }

  const reader = /^reader\.([\w.]+)$/.exec(trimmed);
  if (reader && record.reader_fields.includes(reader[1]          )) {
    return { kind: "reader", name: trimmed, detail: "from the reader, per request" };
  }

  if (record.dimensions.includes(trimmed)) {
    return { kind: "dimension", name: trimmed, detail: "from the preview context" };
  }

  return { kind: "expression", name: trimmed, detail: "an expression over the preview context" };
}

function pathOf(value         , path          )          {
  let at          = value;
  for (const key of path) {
    if (!isRecord(at)) return undefined;
    at = at[key];
  }
  return at;
}

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

const FENCE = /^---[ \t]*\r?\n/;

function parseFrontmatter(text        )                    {
  if (!FENCE.test(text)) return { raw: "", fields: {}, body: text };
  const lines = text.split(/(?<=\n)/);
  let close = -1;
  for (let at = 1; at < lines.length; at += 1) {
    if (/^---[ \t]*\r?\n?$/.test(lines[at]          )) {
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

function readFields(lines          )                             {
  const fields                             = {};
  let at = 0;
  while (at < lines.length) {
    const line = (lines[at]          ).replace(/\r?\n$/, "");
    at += 1;
    if (line.trim() === "" || line.trimStart().startsWith("#")) continue;
    const match = /^([A-Za-z_][\w-]*)\s*:\s*(.*)$/.exec(line);
    if (!match) continue;
    const key = match[1]          ;
    const inline = (match[2]          ).trim();
    if (inline !== "") {
      fields[key] = readScalar(inline);
      continue;
    }
    // A key with nothing after the colon opens a list or a map.
    const nested           = [];
    while (at < lines.length && /^\s+\S/.test((lines[at]          ).replace(/\r?\n$/, ""))) {
      nested.push((lines[at]          ).replace(/\r?\n$/, ""));
      at += 1;
    }
    if (nested.length === 0) {
      fields[key] = null;
    } else if (nested.every((entry) => /^\s*-\s/.test(entry))) {
      fields[key] = nested.map((entry) => readScalar(entry.replace(/^\s*-\s*/, "")));
    } else {
      const map                         = {};
      for (const entry of nested) {
        const pair = /^\s*([A-Za-z_][\w-]*)\s*:\s*(.*)$/.exec(entry);
        if (pair) map[pair[1]          ] = readScalar((pair[2]          ).trim());
      }
      fields[key] = map;
    }
  }
  return fields;
}

function readScalar(written        )         {
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

function writeScalar(value        )         {
  if (value === null) return "";
  if (typeof value !== "string") return String(value);
  if (NEEDS_QUOTES.test(value)) return `"${value.replace(/"/g, '\\"')}"`;
  return value;
}

function writeEntry(key        , value            )         {
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
function writeFrontmatter(text        , changes                                   )         {
  const parsed = parseFrontmatter(text);
  if (parsed.raw === "") {
    const written = Object.entries(changes)
      .filter(([, value]) => value !== null)
      .map(([key, value]) => writeEntry(key, value              ))
      .join("");
    return written === "" ? text : `---\n${written}---\n\n${text.replace(/^\n+/, "")}`;
  }

  const lines = parsed.raw.split(/(?<=\n)/);
  const close = lines.length - 1;
  const out           = [lines[0]          ];
  const applied = new Set        ();

  for (let at = 1; at < close; at += 1) {
    const line = lines[at]          ;
    const match = /^([A-Za-z_][\w-]*)\s*:/.exec(line);
    const key = match?.[1];
    if (key === undefined || !(key in changes)) {
      out.push(line);
      continue;
    }
    applied.add(key);
    const value = changes[key]                     ;
    if (value !== null) out.push(writeEntry(key, value));
    // Whether replaced or removed, the key's continuation lines go with it.
    while (at + 1 < close && /^\s+\S/.test(lines[at + 1]          )) at += 1;
  }

  for (const [key, value] of Object.entries(changes)) {
    if (applied.has(key) || value === null) continue;
    out.push(writeEntry(key, value));
  }
  out.push(lines[close]          );
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
const FIELD_HELP                         = {
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

const ADVANCED = new Set(Object.keys(FIELD_HELP).filter((name) => !COMMON.has(name)));

                                                                                                     

                            
               
                        
                    
               
                                                     
                     
 

                      
                                                      
                                                  
 

/** One field per schema property, in the schema's own order. */
function formFields(schema            )              {
  return Object.entries(schema.properties).map(([name, property]) => {
    const field            = {
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

function controlFor(name        , property                         )               {
  const types = typesOf(property);
  if (types.includes("boolean")) return "boolean";
  if (types.includes("integer") || types.includes("number")) return "number";
  if (types.includes("array")) return "list";
  if (types.includes("object")) return "object";
  if (types.includes("string")) return name === "description" ? "textarea" : "text";
  return "opaque";
}

function typesOf(property                         )           {
  const type = property["type"];
  if (typeof type === "string") return [type];
  if (Array.isArray(type)) return type.map(String);
  return [];
}

function resolved(schema            , property                         )                          {
  const anyOf = property["anyOf"];
  if (!Array.isArray(anyOf)) return property;
  for (const branch of anyOf) {
    if (typeof branch !== "object" || branch === null) continue;
    const reference = (branch                           )["$ref"];
    if (typeof reference !== "string") continue;
    const name = reference.replace("#/$defs/", "");
    const target = schema.$defs?.[name];
    if (target) return target;
  }
  return property;
}

                             
                
               
                  
 

                             
                       
                                                               
                      
                   
 

/**
 * ED-11's save gate.
 *
 * What it does not check it says it did not check. A validator that reports
 * "valid" for a value nothing looked at is the editor asserting something it
 * never established; the build still validates everything, and an unchecked
 * field does not block a save.
 */
function validateFrontmatter(schema            , fields                         )             {
  const errors               = [];
  const unchecked           = [];

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
    if (!types.some((type) => matchesType(type, value))) {
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
      if (!value.every((item) => matchesType(itemType, item))) {
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
function frontmatterDiagnostics(validation            )               {
  return validation.errors.map((error) => ({
    code: error.code,
    severity: "error",
    message: error.message,
    url: `https://kasecrab.github.io/liyasa/docs/errors/${error.code}`,
  }));
}

function matchesType(type        , value         )          {
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

function describe(value         )         {
  if (Array.isArray(value)) return "a list";
  if (value === null) return "null";
  if (isRecord(value)) return "a map";
  return typeof value;
}

// ED-10: create, rename, move, duplicate and delete a page, and drag the
// navigation tree.
//
// Every operation returns a **change** rather than performing one: the files to
// write, move and delete, the new `liyasa.json`, and the redirects to add. The
// editor applies a change as one save, and a change that would leave the
// project unbuildable is refused with the diagnostic the build would have
// raised — E0104 for a navigation entry with no page, E0105 for a duplicate
// route, E0106 for two redirects with the same source. Finding that out at the
// next build instead is the editor handing the author a broken project.


/**
 * The route a path serves.
 *
 * The rule is `liyasa_build::nav::normalize`'s, deliberately: the two have to
 * agree or a redirect the editor writes points at a route the build does not
 * serve.
 */
function routeOf(path        )         {
  const trimmed = path.replace(/^\/+|\/+$/g, "");
  const withoutExtension = trimmed.replace(/\.mdx?$/, "");
  const withoutIndex = withoutExtension.replace(/\/index$/, "");
  return withoutIndex === "" || withoutIndex === "index" ? "/" : `/${withoutIndex}`;
}

/** The navigation tree, whichever of `navigation`'s three forms it is in. */
function navigationOf(config                         )                    {
  const navigation = config["navigation"];
  if (Array.isArray(navigation)) return navigation                     ;
  if (isRecord(navigation)) {
    const tree = navigation["pages"] ?? navigation["tabs"];
    if (Array.isArray(tree)) return tree                     ;
  }
  return [];
}

function withNavigation(config                         , tree                   )                          {
  const navigation = config["navigation"];
  if (isRecord(navigation)) {
    const key = "tabs" in navigation ? "tabs" : "pages";
    return { ...config, navigation: { ...navigation, [key]: tree } };
  }
  return { ...config, navigation: tree };
}

/** `navigation` may name a file; the editor cannot edit what it does not hold. */
function navigationFile(config                         )                {
  const navigation = config["navigation"];
  return typeof navigation === "string" ? navigation : null;
}

function unchanged(project         , diagnostics              )                {
  return { writes: [], moves: [], deletes: [], redirects: [], config: project.config, diagnostics };
}

function error(code        , message        )             {
  return {
    code,
    severity: "error",
    message,
    url: `https://kasecrab.github.io/liyasa/docs/errors/${code}`,
  };
}

const CROCKFORD = "0123456789ABCDEFGHJKMNPQRSTVWXYZ";

/**
 * A ULID, the identifier PRD §6.2.1 gives a page.
 *
 * Page identity is what makes an edge in the truth graph survive a rename, so
 * every new page gets a fresh one and a duplicate never inherits its source's.
 */
function pageId(now         = Date.now())         {
  let time = "";
  let at = now;
  for (let index = 0; index < 10; index += 1) {
    time = (CROCKFORD[at % 32]          ) + time;
    at = Math.floor(at / 32);
  }
  const random = new Uint8Array(16);
  globalThis.crypto.getRandomValues(random);
  let tail = "";
  for (let index = 0; index < 16; index += 1) tail += CROCKFORD[(random[index]          ) % 32];
  return time + tail;
}

function existingRedirects(config                         )                 {
  const redirects = config["redirects"];
  if (Array.isArray(redirects)) return redirects                  ;
  if (isRecord(redirects) && Array.isArray(redirects["rules"])) return redirects["rules"]                  ;
  return [];
}

function withRedirects(config                         , rules                )                          {
  const redirects = config["redirects"];
  if (isRecord(redirects)) return { ...config, redirects: { ...redirects, rules } };
  return { ...config, redirects: rules };
}

function createPage(
  project         ,
  options                                                ,
)                {
  if (options.path in project.pages) {
    return unchanged(project, [
      error("E0105", `\`${options.path}\` already exists; ${routeOf(options.path)} would be served twice.`),
    ]);
  }
  const tree = navigationOf(project.config);
  const at = tree.findIndex((node) => node.group === options.group);
  if (at === -1) {
    return unchanged(project, [
      error("E0104", `there is no navigation group called \`${options.group}\` to put this page in.`),
    ]);
  }
  const text = `---\nid: ${pageId()}\ntitle: ${options.title}\n---\n\n`;
  const next = tree.map((node, index) =>
    index === at ? { ...node, pages: [...node.pages, options.path] } : node,
  );
  return {
    writes: [{ path: options.path, text }],
    moves: [],
    deletes: [],
    redirects: [],
    config: withNavigation(project.config, next),
    diagnostics: [],
  };
}

/**
 * ED-10 in one line: a title change does not change a URL.
 *
 * The file does not move, the slug does not change, and no redirect is needed.
 */
function renamePage(project         , options                                 )                {
  const text = project.pages[options.path];
  if (text === undefined) {
    return unchanged(project, [error("E0104", `\`${options.path}\` is not a page in this draft.`)]);
  }
  return {
    writes: [{ path: options.path, text: writeFrontmatter(text, { title: options.title }) }],
    moves: [],
    deletes: [],
    redirects: [],
    config: project.config,
    diagnostics: [],
  };
}

function movePage(project         , options                              )                {
  const text = project.pages[options.from];
  if (text === undefined) {
    return unchanged(project, [error("E0104", `\`${options.from}\` is not a page in this draft.`)]);
  }
  if (options.to in project.pages) {
    return unchanged(project, [
      error("E0105", `\`${options.to}\` already exists; ${routeOf(options.to)} would be served twice.`),
    ]);
  }

  const source = routeOf(options.from);
  const destination = routeOf(options.to);
  const rules = existingRedirects(project.config);
  const clash = rules.find((rule) => rule.source === source);
  if (clash) {
    return unchanged(project, [
      error(
        "E0106",
        `a redirect from ${source} to ${clash.destination} already exists, so the move cannot add one. ` +
          `Change or remove that rule first.`,
      ),
    ]);
  }

  const added               = { source, destination, status: 301 };
  const tree = navigationOf(project.config).map((node) => ({
    ...node,
    pages: node.pages.map((page) => (page === options.from ? options.to : page)),
  }));
  // The bytes move with the file; the id in particular, which is what keeps
  // every link and every graph edge pointing at this page.
  return {
    writes: [{ path: options.to, text }],
    moves: [{ from: options.from, to: options.to }],
    deletes: [],
    redirects: [added],
    config: withRedirects(withNavigation(project.config, tree), [...rules, added]),
    diagnostics: [],
  };
}

function duplicatePage(project         , options                              )                {
  const text = project.pages[options.from];
  if (text === undefined) {
    return unchanged(project, [error("E0104", `\`${options.from}\` is not a page in this draft.`)]);
  }
  if (options.to in project.pages) {
    return unchanged(project, [
      error("E0105", `\`${options.to}\` already exists; ${routeOf(options.to)} would be served twice.`),
    ]);
  }
  const title = parseFrontmatter(text).fields["title"];
  const copied = writeFrontmatter(text, {
    id: pageId(),
    title: typeof title === "string" ? `${title} (copy)` : "Copy",
  });
  return {
    writes: [{ path: options.to, text: copied }],
    moves: [],
    deletes: [],
    redirects: [],
    config: project.config,
    diagnostics: [],
  };
}

function deletePage(project         , options                  )                {
  if (!(options.path in project.pages)) {
    return unchanged(project, [error("E0104", `\`${options.path}\` is not a page in this draft.`)]);
  }
  const tree = navigationOf(project.config).map((node) => ({
    ...node,
    pages: node.pages.filter((page) => page !== options.path),
  }));
  return {
    writes: [],
    moves: [],
    deletes: [options.path],
    redirects: [],
    config: withNavigation(project.config, tree),
    diagnostics: [],
  };
}

/**
 * A drag in the navigation tree.
 *
 * It moves an entry, never a file: the page keeps its path, its route and its
 * id, so nothing needs a redirect.
 */
function reorderNavigation(
  project         ,
  options                                                    ,
)                {
  const file = navigationFile(project.config);
  if (file !== null) {
    return unchanged(project, [
      error(
        "E0104",
        `this project's navigation lives in \`${file}\`, which this draft does not hold, so the editor cannot reorder it.`,
      ),
    ]);
  }
  const tree = navigationOf(project.config);
  const target = tree.findIndex((node) => node.group === options.toGroup);
  if (target === -1) {
    return unchanged(project, [error("E0104", `there is no navigation group called \`${options.toGroup}\`.`)]);
  }

  const removed = tree.map((node) => ({ ...node, pages: node.pages.filter((page) => page !== options.page) }));
  const destination = removed[target]                   ;
  const pages = [...destination.pages];
  const at = Math.min(Math.max(options.toIndex, 0), pages.length);
  pages.splice(at, 0, options.page);
  const next = removed.map((node, index) => (index === target ? { ...node, pages } : node));

  return {
    writes: [],
    moves: [],
    deletes: [],
    redirects: [],
    config: withNavigation(project.config, next),
    diagnostics: [],
  };
}

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



/** Which segments a replace is allowed to touch. */
                                              

const DEFAULT_SCOPES          = ["text", "props"];

                        
               
                  
                                    
               
                 
                                                 
               
 

                              
               
                 
                
                       
 

                           
                   
                       
                            
 

                                 
               
                      
                   
                  
                          
 

function findReplace(pages               , options                )           {
  const scopes = new Set(options.scopes ?? DEFAULT_SCOPES);
  let pattern        ;
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
          message: `\`${options.find}\` is not a valid regular expression: ${(cause         ).message}`,
          url: "https://kasecrab.github.io/liyasa/docs/errors/E0103",
        },
      ],
    };
  }

  const matches          = [];
  const planned                = [];

  for (const page of pages) {
    const replaced = replaceInPage(page, pattern, options.replaceWith, scopes, matches);
    if (replaced) planned.push(replaced);
  }

  return { matches, pages: planned, diagnostics: [] };
}

/** ED-12's apply: the plan's own text, with nothing left to recompute. */
function applyPlan(plan          )                                   {
  return plan.pages.map((page) => ({ path: page.path, text: page.after }));
}

/** A range of one segment's text a replace may touch, as offsets into it. */
                       
                
              
 

/**
 * The ranges of `text` — one segment's own text — a replace may rewrite.
 *
 * Offsets are relative to the segment, so every segment is rewritten on its
 * own and the page is the concatenation. Nothing has to know how much the
 * segments before it grew, which is where a preview and its edits drift apart.
 */
function replaceable(segment                                    , text        , scopes            )                {
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
    const ranges                = [];
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
  page             ,
  pattern        ,
  replaceWith        ,
  scopes            ,
  matches         ,
)                     {
  const index = byteIndex(page.source);
  const starts = lineStarts(page.source);
  const found          = [];
  const edits                = [];
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
          column: absolute - (starts[line]          ) + 1,
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

function changeFactReference(
  pages               ,
  options                              ,
)           {
  // A fact id in prose is prose. Only the call is a reference, so the pattern
  // is anchored to `fact("...")` and the scope is the template segment that
  // holds it — the one scope a plain replace never touches.
  const pattern = new RegExp(`(fact\\(\\s*["'])${escapeRegExp(options.from)}(["']\\s*\\))`, "g");
  const matches          = [];
  const planned                = [];

  for (const page of pages) {
    const starts = lineStarts(page.source);
    const index = byteIndex(page.source);
    let after = "";
    let copied = 0;
    const edits                = [];

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
        column: start - (starts[line]          ) + 1,
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
function moveGroup(project         , options                                    )                {
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
  next.splice(Math.min(Math.max(options.toIndex, 0), next.length), 0, moved                   );
  const navigation = project.config["navigation"];
  const config = isRecord(navigation)
    ? { ...project.config, navigation: { ...navigation, [("tabs" in navigation ? "tabs" : "pages")]: next } }
    : { ...project.config, navigation: next };
  return { writes: [], moves: [], deletes: [], redirects: [], config, diagnostics: [] };
}

/** ED-12's "apply a tag": front matter only, and only where it changes. */
function applyTag(
  pages                        ,
  options                                  ,
)                                   {
  const writes                                   = [];
  for (const path of options.paths) {
    const text = pages[path];
    if (text === undefined) continue;
    if (parseFrontmatter(text).fields["tag"] === options.tag) continue;
    writes.push({ path, text: writeFrontmatter(text, { tag: options.tag }) });
  }
  return writes;
}

// ED-13: the media library.
//
// What is here is what the browser can decide: the type an upload really is,
// whether it carries alt text, what Markdown an asset becomes, which pages use
// it, and what a crop asks the image endpoint for.
//
// What is **not** here is metadata stripping and SVG checking. Those are
// `liyasa_build::media::accept`'s, on the server, and this module says nothing
// about them — an editor that reported "metadata removed" beside an upload it
// only forwarded would be claiming something it never did. The server's
// refusal comes back as a `Diagnostic` and is shown as it arrives.

                                                                                  

                        
               
              
                   
                
                 
 

                              
                    
                      
                   
 

/** Magic numbers, mirroring `liyasa_build::media::sniff`. */
const SIGNATURES                                           = [
  { bytes: [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a], kind: img("png", "image/png") },
  { bytes: [0xff, 0xd8, 0xff], kind: img("jpeg", "image/jpeg") },
  { bytes: [0x47, 0x49, 0x46, 0x38], kind: img("gif", "image/gif") },
  { bytes: [0x25, 0x50, 0x44, 0x46], kind: { extension: "pdf", contentType: "application/pdf", isImage: false } },
];

function img(extension        , contentType        )              {
  return { extension, contentType, isImage: true };
}

/**
 * What the bytes are, whatever the file is called.
 *
 * An upload named `.png` that is really an SVG is how a script gets served
 * inline from the documentation origin, so the name is never consulted.
 */
function sniff(bytes            )                     {
  for (const { bytes: signature, kind } of SIGNATURES) {
    if (signature.every((byte, at) => bytes[at] === byte)) return kind;
  }
  // RIFF....WEBP
  if (starts(bytes, [0x52, 0x49, 0x46, 0x46]) && starts(bytes.subarray(8), [0x57, 0x45, 0x42, 0x50])) {
    return img("webp", "image/webp");
  }
  // ....ftypavif
  if (bytes.length > 12 && starts(bytes.subarray(4), [0x66, 0x74, 0x79, 0x70, 0x61, 0x76, 0x69, 0x66])) {
    return img("avif", "image/avif");
  }
  const head = new TextDecoder().decode(bytes.subarray(0, 512)).trimStart();
  if (head.startsWith("<svg") || (head.startsWith("<?xml") && head.includes("<svg"))) {
    return img("svg", "image/svg+xml");
  }
  return null;
}

function starts(bytes            , signature          )          {
  return signature.every((byte, at) => bytes[at] === byte);
}

                                  
                   
                    
              
     
                                                                          
                                                                          
                         
     
                       
                        
 

const DEFAULT_ALLOW_TYPES = ["png", "jpeg", "gif", "webp", "avif", "svg", "pdf", "mp4", "webm", "mp3"];

/** What the browser can refuse before spending an upload on it. */
function validateUpload(candidate                 )               {
  const kind = sniff(candidate.bytes);
  if (!kind) {
    return [
      diagnostic(
        "E0812",
        `\`${candidate.filename}\` is not a type Liyasa recognizes. The content is read, not the file name.`,
      ),
    ];
  }
  const allowed = candidate.allowTypes ?? DEFAULT_ALLOW_TYPES;
  if (!allowed.includes(kind.extension)) {
    return [
      diagnostic(
        "E0812",
        `\`${candidate.filename}\` is ${kind.extension}, which \`security.uploads.allowTypes\` does not list.`,
      ),
    ];
  }
  if (kind.isImage && candidate.decorative !== true && candidate.alt.trim() === "") {
    return [
      diagnostic(
        "E0305",
        `\`${candidate.filename}\` needs alt text: what the image says, for a reader who cannot see it. ` +
          `Mark it decorative if it says nothing.`,
      ),
    ];
  }
  return [];
}

                             
              
              
                
                 
                  
                   
 

/**
 * The `::image` directive an asset becomes.
 *
 * `alt` is always written, even when it is empty: an omitted `alt` is an
 * image nobody decided about, and `alt=""` is one somebody marked decorative.
 * The two mean different things to a screen reader and to `E0305`.
 */
function imageDirective(props            )         {
  const parts = [`src="${quote(props.src)}"`, `alt="${quote(props.alt)}"`];
  if (props.dark) parts.push(`dark="${quote(props.dark)}"`);
  if (props.width !== undefined) parts.push(`width=${props.width}`);
  if (props.height !== undefined) parts.push(`height=${props.height}`);
  if (props.caption) parts.push(`caption="${quote(props.caption)}"`);
  return `::image{${parts.join(" ")}}`;
}

function quote(value        )         {
  return value.replace(/\\/g, "\\\\").replace(/"/g, '\\"');
}

/**
 * The dark-mode partner of a light asset, when the project has one.
 *
 * The convention is a `-dark` suffix before the extension. The file has to
 * exist: offering a `dark` prop pointing at nothing is the editor writing a
 * broken reference on the author's behalf.
 */
function darkPartner(path        , assets          )                {
  if (/-dark\.[^.]+$/.test(path)) return null;
  const candidate = path.replace(/(\.[^.]+)$/, "-dark$1");
  return assets.includes(candidate) ? candidate : null;
}

/** The library's search: file name, alt text, and the pages that use it. */
function searchAssets                 (assets     , query        )      {
  const needle = query.trim().toLowerCase();
  if (needle === "") return [...assets];
  return assets.filter((asset) =>
    [asset.path, asset.alt, ...asset.usedOn].some((field) => field.toLowerCase().includes(needle)),
  );
}

/**
 * Why an asset cannot be deleted, or `null`.
 *
 * The message names the pages. "Used on 4 pages" leaves the author hunting for
 * four pages, which is the work the editor is supposed to have done.
 */
function deleteRefusal(asset       )                    {
  if (asset.usedOn.length === 0) return null;
  return diagnostic(
    "E0703",
    `\`${asset.path}\` is used on ${asset.usedOn.length} page${asset.usedOn.length === 1 ? "" : "s"} ` +
      `and cannot be deleted: ${asset.usedOn.join(", ")}. Remove the references first, or replace the file.`,
  );
}

                            
                                                                 
                                               
 

/** The query string the image endpoint answers for a crop and resize. */
function transformQuery(transform           )         {
  const parts           = [];
  if (transform.crop) {
    const { x, y, width, height } = transform.crop;
    if (width <= 0 || height <= 0) {
      throw new Error("a crop needs a width and height above zero");
    }
    parts.push(`crop=${x},${y},${width},${height}`);
  }
  if (transform.resize?.width !== undefined) parts.push(`w=${transform.resize.width}`);
  if (transform.resize?.height !== undefined) parts.push(`h=${transform.resize.height}`);
  return parts.join("&");
}

function diagnostic(code        , message        )             {
  return {
    code,
    severity: "error",
    message,
    url: `https://kasecrab.github.io/liyasa/docs/errors/${code}`,
  };
}

// ED-20 and ED-21: drafts, autosave, and what happens when two saves meet.
//
// The rule that shapes everything here is ED-21's: without real-time
// collaboration (ED-31), **a save whose base version is stale is rejected**.
// Last-write-wins is the easy implementation and it loses somebody's work
// every time two tabs are open, quietly, with both of them reporting a
// successful save. So `save` refuses, hands back the current text, and returns
// the local edit as a suggestion the author accepts or discards.

                        
             
                 
                 
                
                  
                    
                                                     
 

/**
 * ED-20's branch name: `liyasa/<user>/<slug>`.
 *
 * Both parts go through `git check-ref-format`'s rules, because the editor is
 * what names the branch and a draft that cannot be created is the editor's
 * fault, not the author's. Lowercased as well: a ref is a file on macOS and
 * Windows, where `liyasa/Ada/X` and `liyasa/ada/x` are two refs to git and one
 * file to the filesystem — which is one draft silently becoming another.
 */
function draftBranch(user        , slug        )         {
  const parts = [refComponent(user), refComponent(slug)];
  if (parts.some((part) => part === "")) {
    throw new Error(`\`${user}/${slug}\` has nothing a branch can be named after`);
  }
  return `liyasa/${parts[0]}/${parts[1]}`;
}

function refComponent(text        )         {
  return (
    text
      .toLowerCase()
      // The three sequences git names in `check-ref-format` that are not single
      // characters, so they cannot be handled by the class below.
      .replace(/@\{/g, "-")
      .replace(/\.\./g, "-")
      .replace(/\.lock\b/g, "-lock")
      // Everything else git refuses — control characters, a space, `~^:?*[]\`
      // — plus `/`, which would make a second path component out of one name.
      // What is left is the set a ref may hold.
      .replace(/[^a-z0-9._-]+/g, "-")
      .replace(/-+/g, "-")
      // A component may not begin or end with a dot.
      .replace(/^[.-]+/, "")
      .replace(/[.-]+$/, "")
  );
}

/** The drafts list's search: author, branch, title, and pages touched. */
function searchDrafts(drafts         , query        )          {
  const needle = query.trim().toLowerCase();
  if (needle === "") return [...drafts];
  return drafts.filter((draft) =>
    [draft.author, draft.branch, draft.title, ...draft.pages].some((field) =>
      field.toLowerCase().includes(needle),
    ),
  );
}

                             
               
                                         
 

/** The pages a draft touched, each once, in a stable order. */
function pagesTouched(changes              )           {
  return [...new Set(changes.map((change) => change.path))].sort();
}

const MINUTE = 60_000;
const HOUR = 60 * MINUTE;
const DAY = 24 * HOUR;

/** A draft's age, the way the drafts list says it. */
function ageOf(at        , now        )         {
  const elapsed = Math.max(now - at, 0);
  if (elapsed < MINUTE) return "just now";
  if (elapsed < HOUR) return plural(Math.floor(elapsed / MINUTE), "minute");
  if (elapsed < DAY) return plural(Math.floor(elapsed / HOUR), "hour");
  return plural(Math.floor(elapsed / DAY), "day");
}

function plural(count        , unit        )         {
  return `${count} ${unit}${count === 1 ? "" : "s"} ago`;
}

                               
                  
               
 

                          
                      
     
                                     
    
                                                                         
                                                                            
                                                                           
                                                     
     
                   
               
 

                             
               
               
                 
                 
                        
 

                         
                                               
                                                                                 

/**
 * One autosave against the draft the server holds.
 *
 * A base that does not equal the server's version is refused — including one
 * that is *ahead*, which means the client and the server disagree about which
 * draft this is, and accepting it writes one draft's text over another's.
 */
function save(server              , attempt         )              {
  if (attempt.baseVersion === server.version) {
    return { ok: true, version: server.version + 1, text: attempt.text };
  }
  const merged = merge3(attempt.baseText, attempt.text, server.text);
  return {
    ok: false,
    reason: "stale",
    latest: { ...server },
    suggestion: {
      base: attempt.baseText,
      mine: attempt.text,
      theirs: server.text,
      merged: merged.text,
      conflicts: merged.conflicts,
    },
  };
}

                           
                                                                 
               
               
               
                 
 

                        
               
                        
 

/**
 * A three-way merge, line by line.
 *
 * A conflict carries all three texts rather than only a marker-filled string,
 * because ED-21's UI shows both versions *rendered*: it needs each side as a
 * document it can hand to the preview, not as a diff someone has to read.
 */
function merge3(base        , mine        , theirs        )        {
  const baseLines = lines(base);
  const mineLines = lines(mine);
  const theirsLines = lines(theirs);

  const toMine = lcsMatches(baseLines, mineLines);
  const toTheirs = lcsMatches(baseLines, theirsLines);

  // A base line both sides kept is a place the three agree, and the regions
  // between two such lines are what has to be reconciled.
  const anchors                                                   = [];
  let lastMine = -1;
  let lastTheirs = -1;
  for (let at = 0; at < baseLines.length; at += 1) {
    const inMine = toMine.get(at);
    const inTheirs = toTheirs.get(at);
    if (inMine === undefined || inTheirs === undefined) continue;
    if (inMine <= lastMine || inTheirs <= lastTheirs) continue;
    anchors.push({ base: at, mine: inMine, theirs: inTheirs });
    lastMine = inMine;
    lastTheirs = inTheirs;
  }

  const out           = [];
  const conflicts             = [];
  let baseAt = 0;
  let mineAt = 0;
  let theirsAt = 0;

  const region = (toBase        , toMineEnd        , toTheirsEnd        ) => {
    const fromBase = baseLines.slice(baseAt, toBase).join("");
    const fromMine = mineLines.slice(mineAt, toMineEnd).join("");
    const fromTheirs = theirsLines.slice(theirsAt, toTheirsEnd).join("");
    if (fromMine === fromTheirs) {
      out.push(fromMine);
    } else if (fromMine === fromBase) {
      out.push(fromTheirs);
    } else if (fromTheirs === fromBase) {
      out.push(fromMine);
    } else {
      conflicts.push({
        line: countLines(out) + 1,
        base: fromBase,
        mine: fromMine,
        theirs: fromTheirs,
      });
      out.push(`<<<<<<< yours\n${fromMine}=======\n${fromTheirs}>>>>>>> the deploy branch\n`);
    }
  };

  for (const anchor of anchors) {
    region(anchor.base, anchor.mine, anchor.theirs);
    out.push(baseLines[anchor.base]          );
    baseAt = anchor.base + 1;
    mineAt = anchor.mine + 1;
    theirsAt = anchor.theirs + 1;
  }
  region(baseLines.length, mineLines.length, theirsLines.length);

  return { text: out.join(""), conflicts };
}

function lines(text        )           {
  return text === "" ? [] : text.split(/(?<=\n)/);
}

function countLines(chunks          )         {
  let count = 0;
  for (const chunk of chunks) for (const character of chunk) if (character === "\n") count += 1;
  return count;
}

/** Longest common subsequence: base index to other index, for the lines both hold. */
function lcsMatches(left          , right          )                      {
  const table             = Array.from({ length: left.length + 1 }, () =>
    new Array        (right.length + 1).fill(0),
  );
  for (let i = left.length - 1; i >= 0; i -= 1) {
    for (let j = right.length - 1; j >= 0; j -= 1) {
      (table[i]            )[j] =
        left[i] === right[j]
          ? ((table[i + 1]            )[j + 1]          ) + 1
          : Math.max((table[i + 1]            )[j]          , (table[i]            )[j + 1]          );
    }
  }
  const found = new Map                ();
  let i = 0;
  let j = 0;
  while (i < left.length && j < right.length) {
    if (left[i] === right[j]) {
      found.set(i, j);
      i += 1;
      j += 1;
    } else if (((table[i + 1]            )[j]          ) >= ((table[i]            )[j + 1]          )) {
      i += 1;
    } else {
      j += 1;
    }
  }
  return found;
}

/** Which tabs were last heard from on which draft. */
                                                                 

                          
                
              
             
 

function seenTab(registry             , ping         )              {
  return { ...registry, [ping.draft]: { ...(registry[ping.draft] ?? {}), [ping.tab]: ping.at } };
}

/** Longer than this without a ping and a tab is a closed window. */
const TAB_STALE_AFTER = 3 * MINUTE;

/**
 * ED-21's "also open in another tab".
 *
 * A tab that stopped pinging is a window somebody closed, and warning about it
 * teaches the author to ignore the warning that matters.
 */
function alsoOpenElsewhere(registry             , ping         )          {
  const tabs = registry[ping.draft] ?? {};
  return Object.entries(tabs).some(
    ([tab, at]) => tab !== ping.tab && ping.at - at <= TAB_STALE_AFTER,
  );
}

// The HTTP calls the editor makes.
//
// **Almost none of them are served.** `crates/liyasa-server/src/routes/` has
// no `/_liyasa/editor/` route at all: drafts, reviews, previews, the activity
// feed and the agent are WP-14's and WP-16's surfaces and do not exist yet.
// `servedBy` says so per route, `call` refuses an unbuilt one without making a
// request, and the pane that needed it says what is missing rather than
// rendering an empty list.
//
// That last part is the whole point. An editor that asked for drafts, got a
// 404 from a path nobody wired, and drew an empty drafts list would be telling
// an author with twelve drafts that they have none. `web/dashboard/src/api.ts`
// took the same decision one package earlier, for the same reason.

                                                     

                           
             
                                                      
               
                      
                     
 

const API_BASE = "/_liyasa/api/v1";
const EDITOR_BASE = "/_liyasa/editor";

const ENDPOINTS             = [
  // ED-07: the lazy file system the WebAssembly session resolves through.
  { id: "fs.read", method: "GET", path: `${EDITOR_BASE}/fs/{path}`, requirement: "ED-07", servedBy: "unbuilt" },

  // ED-20, ED-21: drafts and their versioned autosave.
  { id: "drafts.list", method: "GET", path: `${EDITOR_BASE}/drafts`, requirement: "ED-20", servedBy: "unbuilt" },
  { id: "drafts.create", method: "POST", path: `${EDITOR_BASE}/drafts`, requirement: "ED-20", servedBy: "unbuilt" },
  { id: "drafts.get", method: "GET", path: `${EDITOR_BASE}/drafts/{id}`, requirement: "ED-20", servedBy: "unbuilt" },
  { id: "drafts.save", method: "PUT", path: `${EDITOR_BASE}/drafts/{id}/files`, requirement: "ED-21", servedBy: "unbuilt" },
  { id: "drafts.tab", method: "POST", path: `${EDITOR_BASE}/drafts/{id}/tab`, requirement: "ED-21", servedBy: "unbuilt" },

  // ED-22: a preview build of the draft.
  { id: "preview.build", method: "POST", path: `${EDITOR_BASE}/drafts/{id}/preview`, requirement: "ED-22", servedBy: "unbuilt" },
  { id: "preview.render", method: "POST", path: `${EDITOR_BASE}/render`, requirement: "ED-07", servedBy: "unbuilt" },

  // ED-23, ED-24, ED-51: review and the publishing policy.
  { id: "review.submit", method: "POST", path: `${EDITOR_BASE}/drafts/{id}/review`, requirement: "ED-23", servedBy: "unbuilt" },
  { id: "review.comments", method: "GET", path: `${EDITOR_BASE}/reviews/{id}/comments`, requirement: "ED-23", servedBy: "unbuilt" },
  { id: "review.comment", method: "POST", path: `${EDITOR_BASE}/reviews/{id}/comments`, requirement: "ED-23", servedBy: "unbuilt" },
  { id: "review.decide", method: "POST", path: `${EDITOR_BASE}/reviews/{id}/decision`, requirement: "ED-51", servedBy: "unbuilt" },
  { id: "policy.get", method: "GET", path: `${EDITOR_BASE}/policy`, requirement: "ED-24", servedBy: "unbuilt" },
  { id: "publish.now", method: "POST", path: `${EDITOR_BASE}/drafts/{id}/publish`, requirement: "ED-24", servedBy: "unbuilt" },

  // ED-25, ED-26: git sync and the workspace that has none.
  { id: "sync.events", method: "GET", path: `${EDITOR_BASE}/events`, requirement: "ED-25", servedBy: "unbuilt" },
  { id: "workspace.revisions", method: "GET", path: `${EDITOR_BASE}/workspace/revisions`, requirement: "ED-26", servedBy: "unbuilt" },
  { id: "workspace.restore", method: "POST", path: `${EDITOR_BASE}/workspace/restore/{revision}`, requirement: "ED-26", servedBy: "unbuilt" },
  { id: "workspace.export", method: "POST", path: `${EDITOR_BASE}/workspace/export`, requirement: "ED-26", servedBy: "unbuilt" },

  // ED-32: the activity feed.
  { id: "activity.feed", method: "GET", path: `${EDITOR_BASE}/activity`, requirement: "ED-32", servedBy: "unbuilt" },

  // ED-40, ED-41, ED-42: the sidebar agent, on the operator's keys.
  { id: "agent.run", method: "POST", path: `${EDITOR_BASE}/agent/run`, requirement: "ED-40", servedBy: "unbuilt" },
  { id: "agent.policy", method: "GET", path: `${EDITOR_BASE}/agent/policy`, requirement: "ED-42", servedBy: "unbuilt" },

  // ED-50, ED-52: the unified review queue and path ownership.
  { id: "queue.list", method: "GET", path: `${EDITOR_BASE}/queue`, requirement: "ED-50", servedBy: "unbuilt" },
  { id: "owners.for", method: "GET", path: `${EDITOR_BASE}/owners`, requirement: "ED-52", servedBy: "unbuilt" },

  // ED-13: the media library's uploads.
  { id: "assets.list", method: "GET", path: `${EDITOR_BASE}/assets`, requirement: "ED-13", servedBy: "unbuilt" },
  { id: "assets.upload", method: "POST", path: `${EDITOR_BASE}/assets`, requirement: "ED-13", servedBy: "unbuilt" },
  { id: "assets.delete", method: "DELETE", path: `${EDITOR_BASE}/assets/{path}`, requirement: "ED-13", servedBy: "unbuilt" },

  // These four exist in `liyasa-server` today.
  { id: "content.tree", method: "GET", path: `${API_BASE}/content`, requirement: "REST-02", servedBy: "wp-14" },
  { id: "builds.trigger", method: "POST", path: `${API_BASE}/builds`, requirement: "GIT-21", servedBy: "wp-16" },
  { id: "builds.status", method: "GET", path: `${API_BASE}/builds/{id}`, requirement: "GIT-21", servedBy: "wp-16" },
  { id: "deployments.current", method: "GET", path: `${API_BASE}/deployments/{env}`, requirement: "REST-01", servedBy: "wp-14" },
];

function findEndpoint(id        )                       {
  return ENDPOINTS.find((endpoint) => endpoint.id === id);
}

/** Fills `{name}` holes, encoding each value. */
function endpointPath(id        , parameters                         = {})         {
  const endpoint = findEndpoint(id);
  if (!endpoint) throw new Error(`no endpoint \`${id}\``);
  return endpoint.path.replace(/\{(\w+)\}/g, (_whole, name        ) => {
    const value = parameters[name];
    if (value === undefined) {
      throw new Error(`\`${id}\` needs ${/^[aeiou]/i.test(name) ? "an" : "a"} \`${name}\``);
    }
    return encodeURIComponent(value);
  });
}

/** Every requirement of this package that is waiting on a handler somewhere. */
function unservedBy()           {
  return [
    ...new Set(
      ENDPOINTS.filter((endpoint) => endpoint.servedBy === "unbuilt").map(
        (endpoint) => endpoint.requirement,
      ),
    ),
  ].sort();
}

                          
                 
                
                  
 

                                                                                 

                           
                              
                 
                                                      
 

/**
 * One call.
 *
 * An endpoint nobody serves fails here, without a request. The refusal names
 * the requirement, so the pane can say "ED-20 is not built in this server"
 * rather than showing an outage or, worse, an empty result.
 */
async function call   (
  id        ,
  spec           = {},
  parameters                         = {},
  fetcher               = fetch,
)                     {
  const endpoint = findEndpoint(id);
  if (!endpoint) return { ok: false, problem: { status: 0, title: `no endpoint \`${id}\`` } };
  if (endpoint.servedBy === "unbuilt") {
    return {
      ok: false,
      problem: {
        status: 501,
        title: "Not served yet",
        detail: `${endpoint.requirement}: no handler answers ${endpoint.method} ${endpoint.path} in this build`,
      },
    };
  }

  const search = new URLSearchParams();
  for (const [name, value] of Object.entries(spec.query ?? {})) {
    if (value !== undefined) search.set(name, String(value));
  }
  const path = endpointPath(id, parameters);
  const url = search.toString() === "" ? path : `${path}?${search}`;

  const headers                         = { accept: "application/json" };
  const init              = { method: spec.method ?? endpoint.method, headers };
  if (spec.body !== undefined) {
    headers["content-type"] = "application/json";
    init.body = JSON.stringify(spec.body);
  }

  try {
    const response = await fetcher(url, init);
    if (!response.ok) {
      const body = (await response.json().catch(() => ({})))                           ;
      return {
        ok: false,
        problem: {
          status: response.status,
          title: typeof body["title"] === "string" ? body["title"] : response.statusText || "Request failed",
          ...(typeof body["detail"] === "string" ? { detail: body["detail"] } : {}),
        },
      };
    }
    return { ok: true, value: (await response.json())      };
  } catch (cause) {
    // A dropped connection is not an empty answer, and drawing one as an empty
    // list tells an author with twelve drafts that they have none.
    return { ok: false, problem: { status: 0, title: "Could not reach the server", detail: String(cause) } };
  }
}

// Generated from `liyasa_server::auth::roles` by `tests/server/ed_75_roles.rs`.
// Do not edit: that test rewrites it and fails when this file and AUTH-30's
// table disagree.

const PERMISSION_NAMES = [
  "dashboardRead",
  "contentDraft",
  "contentPublish",
  "proposalReview",
  "settingsWrite",
  "ownerAct",
]         ;

const ROLE_NAMES = [
  "reader",
  "viewer",
  "contributor",
  "editor",
  "reviewer",
  "admin",
  "owner",
]         ;

const ROLE_PERMISSIONS                           = {
  reader: [],
  viewer: ["dashboardRead"],
  contributor: ["dashboardRead", "contentDraft"],
  editor: ["dashboardRead", "contentDraft", "contentPublish"],
  reviewer: ["dashboardRead", "contentDraft", "proposalReview"],
  admin: ["dashboardRead", "contentDraft", "contentPublish", "proposalReview", "settingsWrite"],
  owner: ["dashboardRead", "contentDraft", "contentPublish", "proposalReview", "settingsWrite", "ownerAct"],
};

// ED-75: what a role may do, and the vocabulary the editor uses for it.
//
// The table is **not** written here. `src/role-table.ts` is generated from
// `liyasa_server::auth::roles` by `tests/server/ed_75_roles.rs`, which fails
// when the two drift. An editor with its own copy of AUTH-30's table
// eventually disagrees with the server, and both ways of disagreeing are bad:
// a Publish button that is enabled and then refused, or one that is disabled
// for somebody the server would have let through.

const PERMISSIONS = PERMISSION_NAMES                         ;
const ROLES = ROLE_NAMES                   ;

function granted(role        )               {
  return (ROLE_PERMISSIONS[role] ?? [])                ;
}

                        
             
                                                         
                                                                             
 

/** Every permission a grant holds, built the way `Grant::permissions` builds it. */
function permissionsOf(grant       )                  {
  const out = new Set            (granted(grant.role));
  for (const custom of grant.custom ?? []) {
    for (const permission of custom.extends ? granted(custom.extends) : []) out.add(permission);
    for (const permission of custom.grant ?? []) out.add(permission);
    for (const permission of custom.revoke ?? []) out.delete(permission);
  }
  return out;
}

function may(grant       , permission            )          {
  return permissionsOf(grant).has(permission);
}

                                                                   

const NEEDED                             = {
  suggest: "contentDraft",
  publish: "contentPublish",
  review: "proposalReview",
  settings: "settingsWrite",
};

/**
 * Why an action is unavailable, in ED-72's words rather than a permission name.
 *
 * `null` means it is available. The refusal names the role the person has and
 * what to ask for, because "you do not have permission" leaves them with
 * nowhere to go.
 */
function refusal(grant       , action        )                {
  if (may(grant, NEEDED[action])) return null;
  const role = grant.role;
  switch (action) {
    case "suggest":
      return `Your account is a ${role}, which can read the documentation but not suggest changes. ` +
        `An administrator can make you a contributor.`;
    case "publish":
      return `Your account is a ${role}, which can suggest changes but not publish them. ` +
        `Submit this for review and someone with publishing rights will take it from there.`;
    case "review":
      return `Your account is a ${role}, which cannot approve or reject a suggestion.`;
    case "settings":
      return `Your account is a ${role}, which cannot change project settings.`;
  }
}

/**
 * What the toolbar's main button says.
 *
 * A contributor never sees "Publish". The whole point of ED-75 is that a
 * company can open editing to every employee, which only works if the button
 * somebody sees is one they can actually press.
 */
function primaryAction(grant       )                                    {
  if (may(grant, "contentPublish")) return { action: "publish", label: "Publish" };
  if (may(grant, "contentDraft")) return { action: "suggest", label: "Submit for review" };
  return { action: "suggest", label: "Suggest an edit" };
}

// ED-23, ED-24, ED-50, ED-51, ED-52: review.
//
// Three decisions worth stating before the code.
//
// **`DOCOWNERS` is `CODEOWNERS`, including the part people get wrong.** The
// last matching rule wins, not the most specific one. An editor that picked
// the most specific rule would assign a different reviewer than the git host
// does for the same file, and the two lists would disagree with nothing to say
// which is right.
//
// **A decision without a reason is refused.** "Rejected" with no reason is a
// decision the author cannot act on and the agent cannot act on either.
//
// **Publish is a plan, not a verb.** ED-24 says branch protection is respected
// *and surfaced*: the editor works out what will happen and says so, rather
// than trying to merge and reporting the host's refusal afterwards.

/** One `DOCOWNERS` file, in file order. */
function parseDocowners(text        )              {
  const rules              = [];
  for (const line of text.split("\n")) {
    const trimmed = line.trim();
    if (trimmed === "" || trimmed.startsWith("#")) continue;
    const [pattern, ...owners] = trimmed.split(/\s+/);
    if (!pattern || owners.length === 0) continue;
    rules.push({ pattern, owners });
  }
  return rules;
}

/**
 * The owners of one path: the **last** matching rule's, or none.
 *
 * Returning none rather than a default is deliberate — the caller decides the
 * fallback, and `assignReviewers` makes the absence explicit.
 */
function ownersFor(rules             , path        )           {
  let found           = [];
  for (const rule of rules) {
    if (matchesPattern(rule.pattern, path)) found = rule.owners;
  }
  return found;
}

function matchesPattern(pattern        , path        )          {
  const normalized = path.replace(/^\/+/, "");
  const cleaned = pattern.replace(/^\/+/, "");
  const source = cleaned
    .split(/(\*\*|\*|\?)/)
    .map((part) => {
      if (part === "**") return ".*";
      if (part === "*") return "[^/]*";
      if (part === "?") return "[^/]";
      return part.replace(/[.+^${}()|[\]\\]/g, "\\$&");
    })
    .join("");
  // A pattern ending in `/` or `/**` covers everything under it; a bare name
  // matches the whole path, the way CODEOWNERS does.
  const anchored = cleaned.includes("/") ? `^${source}$` : `^(?:.*/)?${source}$`;
  return new RegExp(anchored).test(normalized);
}

/**
 * ED-52's assignment: everyone who owns any path the draft touched.
 *
 * The fallback applies per path, only where nothing matched. Adding it to
 * every draft would make it noise that reviewers learn to filter out.
 */
function assignReviewers(
  rules             ,
  paths          ,
  options                        ,
)           {
  const found = new Set        ();
  for (const path of paths) {
    const owners = ownersFor(rules, path);
    if (owners.length > 0) {
      for (const owner of owners) found.add(owner);
      continue;
    }
    if (options.fallback.length === 0) {
      throw new Error(`\`${path}\` has no DOCOWNERS rule and no fallback reviewer`);
    }
    for (const owner of options.fallback) found.add(owner);
  }
  return [...found].sort();
}

                              
             
                      
                      
                            
                                                                            
 

const MILLISECONDS_PER_DAY = 86_400_000;

/**
 * ED-52's stale reminders.
 *
 * A review that is already decided is never reminded; a review reminded
 * recently is not reminded again. Both are the difference between a reminder
 * somebody reads and one they filter.
 */
function staleReminders(
  reviews               ,
  options                                                             ,
)                                        {
  return reviews
    .filter((review) => review.status === "open" || review.status === "changes-requested")
    .filter((review) => options.now - review.requestedAt >= options.afterDays * MILLISECONDS_PER_DAY)
    .filter(
      (review) =>
        review.remindedAt === null || options.now - review.remindedAt >= options.repeatAfterDays * MILLISECONDS_PER_DAY,
    )
    .map((review) => ({ id: review.id, reviewers: [...review.reviewers] }));
}

                                       
                                                           

                            
             
                 
                 
                  
                  
                      
                             
                     
                            
                    
 

                               
                  
                
                    
 

                                             
              
                       
                            
 

/** ED-50's queue, filtered and with the columns the requirement names. */
function queueRows(items             , filters              , now = 0)             {
  return items
    .filter((item) => filters.source === undefined || item.source === filters.source)
    .filter((item) => filters.page === undefined || item.pages.includes(filters.page))
    .filter((item) => filters.reviewer === undefined || item.reviewers.includes(filters.reviewer))
    .map((item) => ({
      ...item,
      age: Math.max(now - item.updatedAt, 0),
      // A link to nowhere is worse than no link: the reviewer clicks it, gets
      // a 404, and concludes the preview is broken rather than absent.
      previewLabel: item.previewUrl === null ? "No preview built" : "Open preview",
      verificationLabel: VERIFICATION_LABEL[item.verification],
    }));
}

const VERIFICATION_LABEL                               = {
  passed: "Checks passed",
  failed: "Checks failed",
  "not-run": "Checks have not run",
};

                                                                    

                           
                     
                
               
             
                  
 

                             
                 
                
                  
             
                        
 

                             
                                                                                                 
                                   

/** ED-51: one decision, recorded, with the publishing policy run on an approval. */
function decide(
  review                                                                    ,
  decision          ,
)                  {
  if (review.status === "merged" || review.status === "rejected") {
    return { ok: false, message: `\`${review.id}\` is already ${review.status}; there is nothing to decide.` };
  }
  if (!may(decision.grant, "proposalReview")) {
    return {
      ok: false,
      message:
        `Your account is a ${decision.grant.role}, which cannot approve or reject a suggestion. ` +
        `Ask a reviewer to take a look.`,
    };
  }
  if (decision.kind !== "approve" && (decision.reason ?? "").trim() === "") {
    return {
      ok: false,
      message:
        decision.kind === "reject"
          ? "A rejection needs a reason: the author cannot act on one without it."
          : "A change request needs a comment saying what to change.",
    };
  }

  const status                        =
    decision.kind === "approve" ? "approved" : decision.kind === "reject" ? "rejected" : "changes-requested";

  return {
    ok: true,
    status,
    audit: {
      action: `review.${decision.kind === "request-changes" ? "requestChanges" : decision.kind}`,
      actor: decision.actor,
      subject: review.id,
      at: decision.at,
      reason: decision.reason?.trim() ?? null,
    },
    runsPublishingPolicy: decision.kind === "approve",
  };
}

                         
                            
                          
                    
 

                              
                                  
                             
 

/**
 * ED-24: what pressing Publish will actually do.
 *
 * Worked out before the click rather than reported after it, so the editor can
 * say "this project requires one approval" instead of showing the host's
 * refusal and leaving the author to interpret it.
 */
function publishPlan(policy        , grant       )              {
  if (!may(grant, "contentPublish")) {
    return {
      action: "open-review",
      explanation:
        `Your account is a ${grant.role}, which can suggest changes but not publish them. ` +
        `This will open a review instead.`,
    };
  }
  if (policy.approvals >= policy.requiredApprovals) {
    return { action: "merge", explanation: null };
  }
  const needed = policy.requiredApprovals;
  return {
    action: "open-review",
    explanation:
      `This project requires ${needed} approval${needed === 1 ? "" : "s"} before a change reaches ` +
      `${policy.protectedBranch}.`,
  };
}

// ED-40, ED-41, ED-42: the sidebar agent.
//
// The rule that shapes this module is ED-41's, and it is absolute: **nothing
// the agent produces is written until a person accepts it.** So an agent run
// returns a `SuggestionSet` — a list of proposed block replacements, each
// pending — and the only function that produces `SegmentEdit`s takes the
// accepted ones. A run cannot write; there is no code path from a model
// response to a file.
//
// ED-42's half is that the agent is given the project's own rules and its
// output is validated before it is offered. A suggestion whose validation has
// errors is never shown: offering a change that does not build, as a change,
// is the editor asking a person to review something it already knows is wrong.

/** The eight operations ED-40 names. */
                       
                
             
                  
              
                 
               
                      
                   

                                
                
                
                                                            
                                                          
                                                           
                      
 

const OPERATIONS                  = [
  { id: "draft-page", label: "Draft a page from a prompt", needs: "page" },
  { id: "rewrite", label: "Rewrite the selection", needs: "selection", variants: ["shorter", "clearer", "tone"] },
  {
    id: "to-component",
    label: "Turn prose into a component",
    needs: "selection",
    variants: ["steps", "tabs", "table"],
  },
  { id: "api-docs", label: "Generate API docs from a spec operation", needs: "spec-operation" },
  { id: "restructure", label: "Restructure this page", needs: "page" },
  { id: "translate", label: "Translate this page", needs: "page" },
  { id: "fix-verification", label: "Fix the verification failures", needs: "page" },
  { id: "explain-diff", label: "Explain this diff", needs: "diff" },
];

function findOperation(id        )                            {
  return OPERATIONS.find((operation) => operation.id === id);
}

/**
 * What ED-42 sends with every run.
 *
 * `AGENTS.md`, the style guide and the verification policy are the project's
 * own rules. An agent that never saw them writes prose the project rejects,
 * and every suggestion becomes work for the reviewer rather than for the
 * agent.
 */
                               
                          
                            
                                    
 

                             
                       
                   
                  
                                                  
                  
                      
 

                                                                   

                                  
             
                                 
                 
                 
                
                                                                  
                    
                           
                                                                                  
                            
 

                                
             
                       
                                 
                                                                               
                              
 

/** The payload a run sends, so a test can see what the agent was told. */
function runPayload(request            )                          {
  const operation = findOperation(request.operation);
  if (!operation) throw new Error(`no agent operation \`${request.operation}\``);
  if (operation.variants && request.variant && !operation.variants.includes(request.variant)) {
    throw new Error(`\`${request.variant}\` is not a variant of \`${operation.id}\``);
  }
  return {
    operation: operation.id,
    ...(request.variant ? { variant: request.variant } : {}),
    ...(request.prompt ? { prompt: request.prompt } : {}),
    scope: [...request.scope],
    rules: {
      agents: request.rules.agentsMd,
      styleGuide: request.rules.styleGuide,
      verification: request.rules.verificationPolicy,
    },
  };
}

/**
 * ED-42: what may be offered, and what is held back.
 *
 * `validate` is the WebAssembly validator the source mode already uses, so the
 * agent's output is held to exactly what the author's typing is held to.
 */
function screen(
  id        ,
  operation           ,
  proposed                                                   ,
  validate                                ,
)                {
  const suggestions                    = [];
  const withheld                    = [];
  for (const candidate of proposed) {
    const diagnostics = validate(candidate.after);
    const row                  = { ...candidate, status: "pending", diagnostics };
    if (diagnostics.some((diagnostic) => diagnostic.severity === "error")) {
      withheld.push(row);
    } else {
      suggestions.push(row);
    }
  }
  return { id, operation, suggestions, withheld };
}

/** One decision on one suggestion. Pure: the set comes back changed. */
function setStatus(set               , id        , status                  )                {
  return {
    ...set,
    suggestions: set.suggestions.map((suggestion) =>
      suggestion.id === id ? { ...suggestion, status } : suggestion,
    ),
  };
}

function acceptAll(set               )                {
  return { ...set, suggestions: set.suggestions.map((suggestion) => ({ ...suggestion, status: "accepted" })) };
}

function rejectAll(set               )                {
  return { ...set, suggestions: set.suggestions.map((suggestion) => ({ ...suggestion, status: "rejected" })) };
}

/**
 * ED-41: the edits for the accepted suggestions, and nothing else.
 *
 * This is the only function in the package that turns an agent's output into
 * something writable, and it reads `status`. A pending set produces no edits,
 * which is what "nothing is written without acceptance" means in code.
 */
function acceptedEdits(model             , set               ) {
  const accepted = set.suggestions.filter((suggestion) => suggestion.status === "accepted");
  if (accepted.length === 0) return [];
  return editBlocks(
    model,
    accepted.map((suggestion) => ({ id: suggestion.target, text: suggestion.after })),
  );
}

/** How the tracked-change gutter counts what is left to decide. */
function pendingCount(set               )         {
  return set.suggestions.filter((suggestion) => suggestion.status === "pending").length;
}

// ED-32: the activity feed.
//
// Five kinds of thing happen to a project and the feed shows all five: drafts,
// reviews, publishes, agent proposals, and verification drift. A feed missing
// one of them is a feed that answers "what changed?" incorrectly, which is
// worse than not having one.

                                                                                 

                           
             
                     
                
                                                             
                  
                  
             
                  
 

/** The five kinds ED-32 names, in the order the filter chips are shown. */
const ACTIVITY_KINDS                 = ["draft", "review", "publish", "proposal", "drift"];

const KIND_LABEL                               = {
  draft: "Drafts",
  review: "Reviews",
  publish: "Publishes",
  proposal: "Agent proposals",
  drift: "Verification drift",
};

                              
                         
                 
                
                 
 

/** The feed, newest first, filtered. */
function feed(entries            , filters              = {})             {
  return entries
    .filter((entry) => filters.kinds === undefined || filters.kinds.includes(entry.kind))
    .filter((entry) => filters.actor === undefined || entry.actor === filters.actor)
    .filter((entry) => filters.page === undefined || entry.pages.includes(filters.page))
    .filter((entry) => filters.since === undefined || entry.at >= filters.since)
    .slice()
    .sort((left, right) => right.at - left.at || left.id.localeCompare(right.id));
}

                           
                                                                                
              
                
                      
 

/**
 * The feed grouped by day.
 *
 * UTC rather than the viewer's zone: two people looking at the same project
 * should see the same groups, and a deploy at 23:40 in one place is not a
 * different day's work from the review that approved it.
 */
function byDay(entries            , now        )             {
  const groups = new Map                    ();
  for (const entry of entries) {
    const day = new Date(entry.at).toISOString().slice(0, 10);
    groups.set(day, [...(groups.get(day) ?? []), entry]);
  }
  const today = new Date(now).toISOString().slice(0, 10);
  const yesterday = new Date(now - 86_400_000).toISOString().slice(0, 10);
  return [...groups.entries()]
    .sort((left, right) => right[0].localeCompare(left[0]))
    .map(([day, rows]) => ({
      day,
      label: day === today ? "Today" : day === yesterday ? "Yesterday" : day,
      entries: rows.slice().sort((left, right) => right.at - left.at),
    }));
}

/** How many of each kind, for the filter chips' counts. */
function counts(entries            )                               {
  const out = { draft: 0, review: 0, publish: 0, proposal: 0, drift: 0 };
  for (const entry of entries) out[entry.kind] += 1;
  return out;
}

// ED-70 and ED-71: getting into the editor from a published page, and the
// guided tasks for people who will use this once a quarter.
//
// Every guided task ends the same way: a **proposal**, reviewed like any
// other. That is the requirement's last clause and it is the point of the
// feature — a task somebody runs once a quarter is exactly the one that should
// not write straight to the published site.



/**
 * ED-70: where "Suggest an edit" on a published page goes.
 *
 * The route comes back as a query rather than a path segment so a route with
 * slashes needs no escaping scheme of its own, and `new` says the editor opens
 * a fresh draft rather than joining whichever one happens to be open.
 */
function suggestEditUrl(route        , options                                  )         {
  const search = new URLSearchParams({ page: route, draft: "new" });
  if (options.block) search.set("block", options.block);
  search.set("branch", draftBranch(options.user, slugOf(route)));
  return `/_liyasa/editor/?${search}`;
}

function slugOf(route        )         {
  const trimmed = route.replace(/^\/+|\/+$/g, "");
  return trimmed === "" ? "home" : trimmed.replace(/\//g, "-");
}

                           
                
                
                 
                
 

/**
 * ED-70's quick fix: one block, edited without leaving the page.
 *
 * It is still a draft and still a proposal — the only thing "quick" removes is
 * the trip to the editor, not the review.
 */
function quickFixProposal(fix          , user        )   
                 
               
                
                
                  
         {
  if (fix.after === fix.before) return null;
  return {
    branch: draftBranch(user, `quick-${slugOf(fix.route)}`),
    page: fix.route,
    block: fix.block,
    after: fix.after,
    summary: `Quick fix on ${fix.route}`,
  };
}

                    
                     
                       
                          
                     
                               

                           
             
                
                                        
                 
                                                         
                
 

/** The five tasks ED-71 names. */
const TASKS             = [
  {
    id: "update-a-number",
    label: "Update a number",
    asks: ["fact", "value"],
    shows: "the pages that show this number",
  },
  {
    id: "rename-everywhere",
    label: "Rename a feature everywhere",
    asks: ["from", "to"],
    shows: "every page and fact the old name appears on",
  },
  {
    id: "replace-a-screenshot",
    label: "Replace a screenshot",
    asks: ["asset", "file"],
    shows: "the pages that use this image",
  },
  { id: "add-a-faq-entry", label: "Add a FAQ entry", asks: ["question", "answer"], shows: "where it will go" },
  {
    id: "record-a-changelog-entry",
    label: "Record a changelog entry",
    asks: ["date", "summary"],
    shows: "the changelog it joins",
  },
];

function findTask(id        )                       {
  return TASKS.find((task) => task.id === id);
}

                           
               
                  
                                                                               
                  
                        
                                                                
                                           
 

/**
 * ED-71's "update a number".
 *
 * The number lives in the fact source, not on the pages — that is the whole
 * reason facts exist. So the proposal writes the fact and *lists* the pages
 * that show it, rather than editing four pages and leaving the fact stale.
 */
function updateANumber(
  pages               ,
  options                                                                       ,
)           {
  const affected = pages
    .filter((page) => page.source.includes(`fact("${options.fact}")`) || page.source.includes(`fact('${options.fact}')`))
    .map((page) => page.path);
  const rewritten = setFactValue(options.factSource, options.fact, options.value);
  return {
    task: "update-a-number",
    summary: `Set ${options.fact} to ${options.value}`,
    pages: affected,
    plan: null,
    writes: rewritten === options.factSource ? [] : [{ path: options.factFile, text: rewritten }],
  };
}

/** The last segment of a dotted fact id, set in a JSON fact file. */
function setFactValue(source        , fact        , value        )         {
  const key = fact.split(".").pop() ?? fact;
  const pattern = new RegExp(`("${escapeRegExp(key)}"\\s*:\\s*)(("(?:[^"\\\\]|\\\\.)*")|[^,\\n}]+)`);
  if (!pattern.test(source)) return source;
  const written = /^-?\d+(\.\d+)?$/.test(value) ? value : JSON.stringify(value);
  return source.replace(pattern, `$1${written}`);
}

/** ED-71's "rename a feature everywhere": pages and facts, with a preview. */
function renameEverywhere(
  pages               ,
  options                                                                      ,
)           {
  const plan = findReplace(pages, { find: options.from, replaceWith: options.to });
  const factPlan = options.factIds ? changeFactReference(pages, options.factIds) : null;
  const touched = new Set(plan.pages.map((page) => page.path));
  for (const page of factPlan?.pages ?? []) touched.add(page.path);
  return {
    task: "rename-everywhere",
    summary: `Rename “${options.from}” to “${options.to}” on ${touched.size} page${touched.size === 1 ? "" : "s"}`,
    pages: [...touched].sort(),
    plan,
    writes: [],
  };
}

/** ED-71's "replace a screenshot": matched by where the old one is used. */
function replaceAScreenshot(
  pages               ,
  options                                        ,
)           {
  const plan = findReplace(pages, { find: options.asset, replaceWith: options.replacement, scopes: ["text", "props"] });
  return {
    task: "replace-a-screenshot",
    summary: `Replace ${options.asset} with ${options.replacement}`,
    pages: plan.pages.map((page) => page.path),
    plan,
    writes: [],
  };
}

/** ED-71's "add a FAQ entry": appended, so the existing entries do not move. */
function addAFaqEntry(
  faq                                  ,
  options                                      ,
)           {
  const entry = `\n## ${options.question.trim()}\n\n${options.answer.trim()}\n`;
  return {
    task: "add-a-faq-entry",
    summary: `Add “${options.question.trim()}” to the FAQ`,
    pages: [faq.path],
    plan: null,
    writes: [{ path: faq.path, text: `${faq.source.replace(/\n*$/, "\n")}${entry}` }],
  };
}

/**
 * ED-71's "record a changelog entry".
 *
 * Prepended under the heading, because a changelog reads newest first and an
 * entry appended to the end is one nobody sees.
 */
function recordAChangelogEntry(
  changelog                                  ,
  options                                   ,
)           {
  if (!/^\d{4}-\d{2}-\d{2}$/.test(options.date)) {
    throw new Error(`\`${options.date}\` is not a date; a changelog entry is dated YYYY-MM-DD`);
  }
  const entry = `## ${options.date}\n\n${options.summary.trim()}\n\n`;
  const lines = changelog.source.split(/(?<=\n)/);
  // After the front matter and the page's own title, before the first entry.
  let at = 0;
  let seenTitle = false;
  for (; at < lines.length; at += 1) {
    const line = lines[at]          ;
    if (/^## /.test(line)) break;
    if (/^# /.test(line)) seenTitle = true;
  }
  if (!seenTitle && at === lines.length) at = lines.length;
  return {
    task: "record-a-changelog-entry",
    summary: `Record the ${options.date} changelog entry`,
    pages: [changelog.path],
    plan: null,
    writes: [
      {
        path: changelog.path,
        text: lines.slice(0, at).join("").replace(/\n*$/, "\n\n") + entry + lines.slice(at).join(""),
      },
    ],
  };
}

// ED-72 and ED-74: the words the editor uses, the tour, contextual help, and
// the page templates.
//
// ED-72's rule is one table and one disclosure. The editor says draft,
// suggest, review, publish and undo; branch, commit, pull request, rebase and
// revert exist under "advanced" for the people who want them. Both halves
// matter — hiding git from somebody who knows git is as unhelpful as showing
// it to somebody who does not — so the mapping is explicit and reversible
// rather than a set of strings scattered through the views.

                       
                
              
                                                    
                   
 

/** ED-72's vocabulary, and the git term behind each word. */
const VOCABULARY                       = {
  draft: {
    plain: "draft",
    git: "branch",
    explains: "A draft is a branch named `liyasa/<you>/<page>`; your changes live there until they are published.",
  },
  suggest: {
    plain: "suggest",
    git: "commit and push",
    explains: "Suggesting saves your changes to your draft's branch and pushes it.",
  },
  review: {
    plain: "review",
    git: "pull request",
    explains: "A review is a pull request against the branch this site is published from.",
  },
  publish: {
    plain: "publish",
    git: "merge and deploy",
    explains: "Publishing merges your draft and starts a deployment, if the project's policy allows it.",
  },
  undo: {
    plain: "undo",
    git: "revert",
    explains: "Undo adds a commit that puts the previous text back; nothing is erased from the history.",
  },
  version: {
    plain: "version",
    git: "commit",
    explains: "Every save is a commit, so any point in a draft's history can be restored.",
  },
};

/** The word to show, given whether the reader asked for the git terms. */
function say(term                                  , advanced         )         {
  const entry = VOCABULARY[term];
  if (!entry) return term;
  return advanced ? `${entry.plain} (${entry.git})` : entry.plain;
}

                           
             
                 
                
               
 

/** ED-74's onboarding tour: six steps, each pointing at something on screen. */
const TOUR             = [
  {
    id: "editing",
    target: "[data-editor-surface]",
    title: "This is your page",
    body: "Type as you would anywhere else. Markdown shortcuts work: `## ` starts a heading, `- ` starts a list.",
  },
  {
    id: "slash",
    target: "[data-editor-surface]",
    title: "Add a callout, a table, an image",
    body: "Press `/` anywhere to insert a component without leaving the keyboard.",
  },
  {
    id: "modes",
    target: "[data-mode-switch]",
    title: "Two ways to look at the same page",
    body: "Visual mode and source mode edit the same file. Switching between them changes nothing.",
  },
  {
    id: "preview",
    target: "[data-preview]",
    title: "What readers will see",
    body: "The preview is built by the same code that builds the site, so it is not an approximation.",
  },
  {
    id: "drafts",
    target: "[data-drafts]",
    title: "Nothing is live yet",
    body: "Every change goes into a draft. Publishing is a separate step, and it may need a review first.",
  },
  {
    id: "help",
    target: "[data-help]",
    title: "Help, whenever",
    body: "Press `?` for the keyboard shortcuts, or open this menu for the rest.",
  },
];

                               
             
                
                                                                            
                                                                      
                      
                                                               
               
 

/**
 * ED-74's page templates: one per documentation type, filled with guidance.
 *
 * The guidance is *in the page* as prose the author replaces, not as a comment
 * they delete. Somebody who has never written documentation needs to see what
 * a good section looks like, and an empty page with a heading teaches nothing.
 */
const TEMPLATES                 = [
  {
    id: "tutorial",
    label: "Tutorial",
    kind: "tutorial",
    description: "A lesson that takes a beginner from nothing to a first success.",
    body: [
      "## What you will build",
      "",
      "One sentence naming the thing that will exist at the end. A reader decides",
      "here whether this is the page they want.",
      "",
      "## Before you start",
      "",
      "- What must already be installed",
      "- What access is needed",
      "",
      "::::steps",
      "",
      ':::step{title="The first step"}',
      "One action, one result. Show the command and what it prints.",
      ":::",
      "",
      ':::step{title="The second step"}',
      "Keep each step small enough to check.",
      ":::",
      "",
      "::::",
      "",
      "## What to do next",
      "",
      "Two or three links, chosen rather than listed.",
      "",
    ].join("\n"),
  },
  {
    id: "how-to",
    label: "How-to guide",
    kind: "how-to",
    description: "Steps for somebody who already knows what they want.",
    body: [
      "## Goal",
      "",
      "One sentence, in the reader's words, naming the task.",
      "",
      "## Steps",
      "",
      "1. The first thing to do",
      "2. The second thing to do",
      "",
      ':::note{title="If this fails"}',
      "The common failure, and what it means.",
      ":::",
      "",
    ].join("\n"),
  },
  {
    id: "reference",
    label: "Reference",
    kind: "reference",
    description: "What something is, exhaustively and without narrative.",
    body: [
      "## Summary",
      "",
      "What this is, in one sentence.",
      "",
      "## Fields",
      "",
      "| Name | Type | Default | What it does |",
      "| --- | --- | --- | --- |",
      "|  |  |  |  |",
      "",
      "## Examples",
      "",
      "```json",
      "{}",
      "```",
      "",
    ].join("\n"),
  },
  {
    id: "explanation",
    label: "Explanation",
    kind: "explanation",
    description: "Why something works the way it does.",
    body: [
      "## The short answer",
      "",
      "Two sentences for a reader who will not read the rest.",
      "",
      "## Why it works this way",
      "",
      "The constraint, the alternatives, and why this one.",
      "",
      "## What this means in practice",
      "",
      "The consequence the reader will actually meet.",
      "",
    ].join("\n"),
  },
  {
    id: "changelog",
    label: "Changelog entry",
    kind: "other",
    description: "One dated entry for the changelog.",
    body: [
      "## YYYY-MM-DD",
      "",
      "What changed, from the reader's side rather than the commit's. \"Rate limits",
      "are now per key rather than per account\" is an entry; \"refactored the",
      "limiter\" is not.",
      "",
      "If a reader has to do something, say so here and link to the page that",
      "explains it.",
      "",
    ].join("\n"),
  },
  {
    id: "api-endpoint",
    label: "API endpoint",
    kind: "reference",
    description: "One operation from an API description.",
    body: [
      "## Summary",
      "",
      "What this operation does, in one sentence.",
      "",
      "## Request",
      "",
      "Describe each parameter that is not obvious from its name.",
      "",
      "## Response",
      "",
      "What comes back, and what an error looks like.",
      "",
    ].join("\n"),
  },
];

function findTemplate(id        )                           {
  return TEMPLATES.find((template) => template.id === id);
}

/** A new page from a template, with its front matter filled in. */
function pageFromTemplate(id        , options                                   )         {
  const template = findTemplate(id);
  if (!template) throw new Error(`no page template \`${id}\``);
  return `---\nid: ${options.pageId}\ntitle: ${options.title}\n---\n\n${template.body}`;
}

                            
             
                
               
 

/** ED-74's contextual help, keyed by what the author is looking at. */
const HELP                            = {
  frontmatter: {
    id: "frontmatter",
    title: "Page settings",
    body: "The fields at the top of a page: its title, its description, and who may see it. The ones most pages use are shown; the rest are under Advanced.",
  },
  templating: {
    id: "templating",
    title: "Values that change",
    body: "`{{ }}` shows a value the project keeps in one place, such as a price or a version. Changing it there changes every page that shows it.",
  },
  components: {
    id: "components",
    title: "Components",
    body: "Callouts, tabs, steps and cards. Press `/` to insert one; select it to change its settings.",
  },
  review: {
    id: "review",
    title: "Review",
    body: "Somebody else reads your suggestion and either approves it or asks for changes. The project decides who, and whether it is required.",
  },
};

// ED-80: WCAG 2.2 AA in the parts of the editor that are this module's to
// decide — what is announced, what is focusable, what a keyboard reaches, and
// what moves when the viewer has asked for less motion.
//
// The parts that are not here are in the markup and the stylesheet, and
// `web/e2e/a11y/editor.spec.ts` drives axe over the running page. What this
// module holds is the decisions a test can check without a browser: the
// announcement text, the keyboard map, and the rule that every action has a
// keyboard route.

                                                

                               
                  
                         
 

/**
 * What a screen reader hears when a save happens.
 *
 * Autosave is `polite`: it happens every few seconds and interrupting somebody
 * mid-sentence to say "saved" makes the editor unusable with a screen reader.
 * A *failed* save is `assertive`, because the alternative is losing work
 * silently.
 */
function saveAnnouncement(state                                         )               {
  switch (state) {
    case "saving":
      return { message: "Saving", politeness: "polite" };
    case "saved":
      return { message: "Saved", politeness: "polite" };
    case "failed":
      return { message: "Not saved. Your changes are still here; check your connection.", politeness: "assertive" };
    case "stale":
      return {
        message: "Not saved. This draft changed somewhere else; your edit is waiting for you to accept or discard it.",
        politeness: "assertive",
      };
  }
}

/** What a screen reader hears when validation finishes. */
function validationAnnouncement(errors        , warnings        )               {
  if (errors === 0 && warnings === 0) return { message: "No problems found", politeness: "polite" };
  const parts           = [];
  if (errors > 0) parts.push(`${errors} problem${errors === 1 ? "" : "s"}`);
  if (warnings > 0) parts.push(`${warnings} suggestion${warnings === 1 ? "" : "s"}`);
  return {
    message: `${parts.join(" and ")}. Press F8 to move through them.`,
    // A problem the author introduced is worth interrupting for; a warning is
    // not, and announcing every warning assertively trains people to ignore
    // the live region.
    politeness: errors > 0 ? "assertive" : "polite",
  };
}

/** What a screen reader hears when a review's state changes. */
function reviewAnnouncement(state                                                              )               {
  const messages                               = {
    submitted: "Submitted for review",
    approved: "Approved. Publishing follows the project's policy.",
    "changes-requested": "Changes requested. The comments are in the review pane.",
    published: "Published",
  };
  return { message: messages[state], politeness: "polite" };
}

                           
               
                 
                      
                                  
                                        
 

/**
 * Every action the editor offers, and the keys that reach it.
 *
 * ED-80 asks for full keyboard operation of the block editor, the properties
 * forms, the review diffs and the charts. That is a statement about *coverage*
 * — so `actionsWithoutShortcut` exists, and its test is what makes the claim
 * checkable rather than aspirational.
 */
const SHORTCUTS             = [
  { keys: "?", action: "show-shortcuts", description: "Show this list", scope: "global" },
  { keys: "Mod+S", action: "save", description: "Save now", scope: "global" },
  { keys: "Mod+Z", action: "undo", description: "Undo", scope: "editor" },
  { keys: "Mod+Shift+Z", action: "redo", description: "Redo", scope: "editor" },
  { keys: "Mod+E", action: "toggle-mode", description: "Switch between visual and source mode", scope: "global" },
  { keys: "Mod+K", action: "command-palette", description: "Search commands and pages", scope: "global" },
  { keys: "/", action: "slash-menu", description: "Insert a component", scope: "editor" },
  { keys: "Mod+Enter", action: "submit-for-review", description: "Submit for review", scope: "global" },
  { keys: "F8", action: "next-problem", description: "Go to the next problem", scope: "global" },
  { keys: "Shift+F8", action: "previous-problem", description: "Go to the previous problem", scope: "global" },
  { keys: "Alt+ArrowUp", action: "move-block-up", description: "Move this block up", scope: "editor" },
  { keys: "Alt+ArrowDown", action: "move-block-down", description: "Move this block down", scope: "editor" },
  { keys: "Mod+Alt+P", action: "open-properties", description: "Open this block's settings", scope: "editor" },
  { keys: "Escape", action: "close-panel", description: "Close the open panel", scope: "global" },
  { keys: "Mod+.", action: "quick-fix", description: "Apply the suggested fix", scope: "editor" },
  { keys: "J", action: "next-suggestion", description: "Next suggested change", scope: "review" },
  { keys: "K", action: "previous-suggestion", description: "Previous suggested change", scope: "review" },
  { keys: "A", action: "accept-suggestion", description: "Accept this suggestion", scope: "review" },
  { keys: "R", action: "reject-suggestion", description: "Reject this suggestion", scope: "review" },
  { keys: "Mod+ArrowRight", action: "next-diff-file", description: "Next file in the diff", scope: "review" },
  { keys: "Mod+ArrowLeft", action: "previous-diff-file", description: "Previous file in the diff", scope: "review" },
  { keys: "T", action: "chart-as-table", description: "Show this chart as a table", scope: "review" },
];

/** Every action the editor's toolbars and panes can perform. */
const ACTIONS           = SHORTCUTS.map((shortcut) => shortcut.action);

/** Actions with no keyboard route. ED-80 requires this to be empty. */
function actionsWithoutShortcut(actions          )           {
  const reachable = new Set(SHORTCUTS.map((shortcut) => shortcut.action));
  return actions.filter((action) => !reachable.has(action));
}

/** Two shortcuts on one key in one scope is one action nobody can reach. */
function shortcutCollisions()           {
  const seen = new Map                ();
  const clashes           = [];
  for (const shortcut of SHORTCUTS) {
    const key = `${shortcut.scope}:${shortcut.keys.toLowerCase()}`;
    const existing = seen.get(key);
    if (existing) clashes.push(`${shortcut.keys} is both ${existing} and ${shortcut.action}`);
    else seen.set(key, shortcut.action);
  }
  return clashes;
}

/**
 * How long a transition may run.
 *
 * `reduced` is zero rather than short: `prefers-reduced-motion` is set by
 * people for whom movement causes nausea or seizures, and a fast animation is
 * still an animation.
 */
function motionDuration(base        , reduced         )         {
  return reduced ? 0 : base;
}

/**
 * Whether a focus ring is drawn.
 *
 * Always, when the element has focus. Hiding it for pointer users is the most
 * common WCAG 2.4.7 failure and it is invisible to whoever wrote the CSS,
 * because they were using a mouse at the time.
 */
function focusVisible()          {
  return true;
}

                           
               
                
 

/** The regions a screen reader user moves between with one keystroke. */
const LANDMARKS             = [
  { role: "banner", label: "Editor toolbar" },
  { role: "navigation", label: "Pages" },
  { role: "main", label: "Page content" },
  { role: "complementary", label: "Preview" },
  { role: "complementary", label: "Problems" },
  { role: "contentinfo", label: "Draft status" },
];

/**
 * A chart's data as a table, for ED-80's "charts with data-table alternatives".
 *
 * The table is built from the same series the chart draws, so it cannot show
 * different numbers.
 */
function chartTable(
  series                                                         ,
)                                                     {
  const columns = ["", ...series.map((line) => line.label)];
  const keys           = [];
  for (const line of series) for (const point of line.points) if (!keys.includes(point.x)) keys.push(point.x);
  const rows = keys.map((key) => [
    key,
    ...series.map((line) => line.points.find((point) => point.x === key)?.y ?? 0),
  ]);
  return { columns, rows };
}

// ED-73: what a validation failure says to somebody who does not write code,
// and what the editor can do about it.
//
// The text lives here rather than in `codes.toml` — RFC 2432 records why: the
// registry has no plain-language field, no fix marker, and is append-only for
// every package. `test/messages.test.ts` reads `codes.toml` itself and asserts,
// in both directions, that this table covers every code an editor-reachable
// crate raises and names no code the registry does not have.
//
// **Nothing here is generated.** An earlier version imported a `code-list.ts`
// generated from the registry and pinned by a Rust test; RFC 2433 records why
// that was wrong. The short version: the only thing the editor wanted the
// registry for at run time was a title to show when a code has no entry below,
// and a `Diagnostic` already carries its own `message`, which is more specific
// than the registry title for exactly the codes that reach this fallback.
//
// Three rules the wording follows:
//
//   * Say what is wrong with the *page*, not what the parser did. "Undefined
//     template variable" is a parser's sentence; "This page uses a value
//     called `plan` that the project does not define" is the author's.
//   * Name the thing the author typed. A message that does not contain the
//     word they wrote is a message they cannot act on.
//   * Offer a fix only where the editor can really perform one. A button that
//     opens a dialog and says "now do it yourself" is worse than no button.

/** What pressing the fix button does. Every one is something the editor can do. */
                       
                 
                      
                  
                     
                            
                       
                         
                          
                       
                      
                     
                          
                        

                               
                
                                             
 

const MESSAGES                               = {
  // Configuration — liyasa-config
  E0101: { plain: "The project's settings file is not readable. Something in `liyasa.json` is not valid JSON.", fix: { label: "Open settings", action: "open-config" } },
  E0102: { plain: "A setting has the wrong kind of value. The field below says what was expected.", fix: { label: "Open settings", action: "open-config" } },
  E0103: { plain: "This setting is not one Liyasa knows. Check the spelling, or remove it.", fix: { label: "Open settings", action: "open-config" } },
  E0104: { plain: "The navigation points at a page that is not in this project. It was probably renamed or deleted.", fix: { label: "Open navigation", action: "open-navigation" } },
  E0105: { plain: "Two pages would be published at the same address. One of them needs a different slug.", fix: { label: "Open navigation", action: "open-navigation" } },
  E0106: { plain: "Two redirects send the same address to different places, so neither can be used.", fix: { label: "Open settings", action: "open-config" } },
  E0107: { plain: "These two colours are too close together for people with low vision to read one against the other.", fix: { label: "Open settings", action: "open-config" } },
  E0108: { plain: "This project has versions or languages but has not said which one readers see first.", fix: { label: "Open settings", action: "open-config" } },
  E0109: { plain: "This redirect points at another website, which has to be listed as allowed first.", fix: { label: "Open settings", action: "open-config" } },
  E0110: { plain: "This setting is not in the project's schema, so the build would not know what to do with it.", fix: { label: "Open settings", action: "open-config" } },
  E0120: { plain: "A private site has to be served by Liyasa itself; it cannot be published as plain files.", fix: { label: "Open settings", action: "open-config" } },
  E0121: { plain: "This project's settings were written by a newer version of Liyasa than the one running." },
  E0132: { plain: "This is not a colour Liyasa can read. Try a hex value such as `#0a84ff`.", fix: { label: "Open settings", action: "open-config" } },
  E0133: { plain: "This part of the navigation is tied to a version, language or product the project has not declared.", fix: { label: "Open navigation", action: "open-navigation" } },
  E0135: { plain: "This API description is fetched from an address the published settings do not list.", fix: { label: "Open settings", action: "open-config" } },

  // Templating and the Source Document — liyasa-markdown
  E0201: { plain: "This page uses a value the project does not define. Check the spelling, or add it to the project's variables." },
  E0202: { plain: "There is a mistake in the `{{ }}` or `{% %}` here — usually a missing brace or quote." },
  E0203: { plain: "This page calls something that does not exist. Check the spelling against the list of available functions." },
  E0204: { plain: "A loop or an include on this page produced more than Liyasa will build \u2014 too many rows, too much text, or nested too deeply." },
  E0205: { plain: "This page includes a file that is not in the project. It was probably renamed or moved." },
  E0206: { plain: "These snippets include each other in a circle, so there is no end to the page." },
  E0207: { plain: "This snippet needs a value that was not given, or was given as the wrong kind of thing." },
  E0208: { plain: "This page reads something about the person viewing it, so it has to be marked personalised first.", fix: { label: "Mark personalised", action: "declare-personalized" } },
  E0209: { plain: "This page refers to a number that is not in the project's facts. Check the name, or add the fact." },
  E0210: { plain: "A `{% for %}` or `{% if %}` opens inside one part of the page and closes inside another, so the page has no clear shape." },
  E0211: { plain: "This page reads a setting from the machine that builds it, which has to be allowed in the project's settings first.", fix: { label: "Open settings", action: "open-config" } },
  E0212: { plain: "There is an invisible character here that Liyasa reserves for its own use. Delete it and retype the line.", fix: { label: "Remove the character", action: "escape-character" } },
  E0213: { plain: "This link points at a page that is not in the project." },
  E0214: { plain: "This page refers to a file the build did not produce. Check the path, or upload the file." },
  E0215: { plain: "This page documents an API operation that is not in the API description." },
  E0216: { plain: "This page asks for something only a full build can supply, so the preview cannot show it." },
  E0301: { plain: "A code block was opened and never closed. Add the closing ``` line.", fix: { label: "Close the code block", action: "close-container" } },
  E0303: { plain: "This project does not allow raw HTML in pages. Use a component instead." },
  E0304: { plain: "This HTML is not on the list of tags and attributes Liyasa publishes, so it was removed." },
  E0305: { plain: "This image is missing a description for screen readers. Say what the image shows, not that it is a screenshot.", fix: { label: "Add a description", action: "add-alt-text" } },
  E0307: { plain: "This page is too long to build. Split it into several pages." },
  E0310: { plain: "A `:::` block was opened and never closed. Add the closing `:::` line.", fix: { label: "Close the block", action: "close-container" } },
  E0311: { plain: "There is a closing `:::` here with nothing above it that it closes.", fix: { label: "Remove it", action: "remove-directive-close" } },
  E0312: { plain: "There is a mistake in this block's settings — usually a missing quote or equals sign." },
  E0313: { plain: "There is no component by this name in the project.", fix: { label: "Use the suggested name", action: "rename-to-suggestion" } },
  E0314: { plain: "This component needs a setting that was not given.", fix: { label: "Add it", action: "add-required-prop" } },
  E0315: { plain: "This setting was given as the wrong kind of value — a number where text was expected, or the other way round." },
  E0317: { plain: "A `:::` block cannot sit in the middle of a sentence; it has to start on its own line." },
  E0318: { plain: "Two blocks on this page were given the same `{#id}`, so links to it would be ambiguous." },
  E0320: { plain: "A value from outside the project has a line break in it, which would break the page apart. It was refused." },
  E0322: { plain: "This page nests things inside each other more deeply than Liyasa will read. Flatten part of it." },

  // Components — liyasa-components
  E0350: { plain: "This component does not have a part by that name." },
  E0351: { plain: "There is a mistake inside the project's own component, not on this page." },
  E0352: { plain: "One of the project's own components describes its settings in a way Liyasa cannot read." },
  E0353: { plain: "This link uses an address type Liyasa will not publish, such as `javascript:`." },
  E0354: { plain: "This block cannot go inside the one around it." },
  E0356: { plain: "This file in the project's components folder is not a component Liyasa can read." },
  E0357: { plain: "This embed is from a site the project has not allowed.", fix: { label: "Open settings", action: "open-config" } },

  // Search — liyasa-search
  E1002: { plain: "This search index was built by a newer version of Liyasa than the one reading it." },
  E1003: { plain: "The search index is damaged or incomplete. Building the site again will rebuild it." },
  E1004: { plain: "This search is not something Liyasa can read — usually an unclosed quote." },
  E1006: { plain: "Searching this language needs a dictionary that is not installed." },

  // Editor and WebAssembly — liyasa-wasm
  E1200: { plain: "This editing session could not start. Reload the page to get a new one." },

  W0130: { plain: "This page is published but nothing links to it from the navigation, so readers have no way to find it.", fix: { label: "Add to navigation", action: "open-navigation" } },
  W0131: { plain: "The project has not said what its address is, so links shared elsewhere may not resolve.", fix: { label: "Open settings", action: "open-config" } },
  W0134: { plain: "This setting was read from the published branch rather than from this draft, so a change here will not take effect until it is published." },
  W0136: { plain: "The project's address already includes its sub-path, so every link would repeat it.", fix: { label: "Open settings", action: "open-config" } },
  W0302: { plain: "This code block has a setting Liyasa does not recognise, so it was ignored.", fix: { label: "Remove it", action: "remove-unknown-prop" } },
  W0306: { plain: "This heading skips a level, which makes the page harder to navigate with a screen reader.", fix: { label: "Fix the level", action: "fix-heading-level" } },
  W0308: { plain: "This page is getting long. Readers and assistants both do better with shorter pages." },
  W0316: { plain: "This component does not have a setting by that name, so it was ignored.", fix: { label: "Remove it", action: "remove-unknown-prop" } },
  W0319: { plain: "The text here looks like one of Liyasa's internal markers, so it was shown literally rather than acted on." },
  W0321: { plain: "Most of this page is generated data rather than prose, which readers and assistants both struggle with." },
  W0355: { plain: "This setting had no effect because another setting on the same block takes precedence.", fix: { label: "Remove it", action: "remove-unknown-prop" } },
  W0358: { plain: "This value is outside the range the component documents, so it may not look the way you expect." },
  W1001: { plain: "Search does not have word-stemming rules for this language, so it will match whole words only." },
  W1005: { plain: "A search setting names pages that do not exist, so it does nothing.", fix: { label: "Open settings", action: "open-config" } },
  W1201: { plain: "This page is too big to preview here, so the preview is being built on the server instead.", fix: { label: "Preview on the server", action: "preview-on-server" } },
};

/**
 * The plain-language message for a code, or `null`.
 *
 * `null` rather than a guess. A caller that has a `Diagnostic` in hand has its
 * `message` too, and showing that is better than the editor inventing a
 * friendly sentence for a code nobody wrote one for.
 */
function messageFor(code        )                      {
  return MESSAGES[code] ?? null;
}

/**
 * What the problems pane shows for one diagnostic.
 *
 * The headline is ED-73's plain language when there is any, and the
 * diagnostic's own message otherwise. `detail` is the diagnostic's message
 * when a plain headline is showing and it adds something the headline cannot —
 * which field, which prop, which file.
 */
function shownFor(diagnostic                                   )   
                   
                        
                                  
                    
  {
  const plain = messageFor(diagnostic.code);
  const message = diagnostic.message.trim();
  if (!plain) {
    return { headline: message === "" ? diagnostic.code : message, detail: null, fix: null, hasPlain: false };
  }
  return {
    headline: plain.plain,
    detail: message === "" || message === plain.plain ? null : message,
    fix: plain.fix ?? null,
    hasPlain: true,
  };
}

// The WebAssembly session, and ED-07's resolve loop.
//
// WP-24a's decision, which this module implements rather than relitigates: the
// module never calls back into JavaScript. A response carries
// `missing: string[]` — paths the draft named that the session does not hold —
// and the host fetches each, hands it over with `seed`, and calls again. A
// synchronous callback from wasm into JS needs `XMLHttpRequest` or
// `Atomics.wait` over a `SharedArrayBuffer`, and neither is something a module
// should require of the page that embeds it.
//
// The loop is bounded twice. A server that keeps answering with a file that
// still leaves the path missing — a redirect loop, a stale cache, a path the
// draft names two ways — would otherwise spin forever on a keystroke.

             
               
                
                 
                  
                   
                    
                  
                   
                                                         

/** The part of `liyasa-wasm`'s `Session` this module drives. */
                              
                                         
                                              
                                                    
                                                       
                                                          
 

                           
                                                                         
                                             
 

/** How many rounds of seeding a single call may take. */
const MAX_ROUNDS = 8;

                              
              
                                                                        
                       
                 
 

                      
                    
 

/**
 * Runs `call`, seeding whatever it says is missing, until it stops asking.
 *
 * Two bounds, and both have caught something in a browser at some point in
 * every editor that has this shape:
 *
 *   * `MAX_ROUNDS`, so a server that answers every fetch without resolving the
 *     path cannot spin on a keystroke.
 *   * a path asked for twice is not fetched twice. A resolver that returns
 *     content the session does not accept as that path would otherwise make no
 *     progress while looking like it was.
 */
async function resolving                      (
  session             ,
  resolver          ,
  call         ,
)                       {
  const asked = new Set        ();
  const unresolved           = [];

  for (let round = 1; round <= MAX_ROUNDS; round += 1) {
    const response = call();
    const wanted = response.missing.filter((path) => !asked.has(path));
    if (wanted.length === 0) {
      // Whatever is still missing here was asked for and did not resolve —
      // the resolver answered, and the session did not accept the answer as
      // that path. Returning an empty `unresolved` would report success on a
      // page that still cannot render.
      return {
        response,
        unresolved: [...new Set([...unresolved, ...response.missing])],
        rounds: round,
      };
    }
    for (const path of wanted) {
      asked.add(path);
      const text = await resolver.read(path);
      if (text === null) {
        unresolved.push(path);
        continue;
      }
      session.seed(path, text);
    }
  }

  // The bound was reached. The last answer is still the best one available and
  // the caller is told how it got there rather than being handed a silent
  // partial render.
  const response = call();
  return {
    response,
    unresolved: [...new Set([...unresolved, ...response.missing])],
    rounds: MAX_ROUNDS,
  };
}

/**
 * The editor's four calls, each with the resolve loop around it.
 *
 * `serialize` has none: it reads no file the session does not already hold,
 * and its response has no `missing` to answer.
 */
class EditorSession {
  session             ;
  resolver          ;

  constructor(session             , resolver          ) {
    this.session = session;
    this.resolver = resolver;
  }

  parse(request              )                                   {
    return resolving(this.session, this.resolver, () => this.session.parse(request));
  }

  preview(request                )                                     {
    return resolving(this.session, this.resolver, () => this.session.preview(request));
  }

  validate(request                 )                   {
    return this.session.validate(request);
  }

  serialize(request                  )                    {
    return this.session.serialize(request);
  }
}

/**
 * A 32-character hexadecimal session nonce.
 *
 * WP-24a makes this required and gives it no default: it is what makes a
 * directive marker unforgeable, and a predictable nonce lets an author type a
 * marker into a page and have the preview parse it as a component nobody
 * declared. Generated here from the platform's CSPRNG, per session.
 */
function sessionNonce()         {
  const bytes = new Uint8Array(16);
  globalThis.crypto.getRandomValues(bytes);
  return [...bytes].map((byte) => byte.toString(16).padStart(2, "0")).join("");
}

/**
 * The editor holds the last good HTML while diagnostics are non-empty.
 *
 * WP-24a's note says so explicitly: a preview expands at `Undefined::Strict`,
 * so a half-typed `{{ ` blanks the render and returns `E0201`, deliberately.
 * Showing that blank would make the preview flash empty on every keystroke
 * inside an expression. Holding the last good render is the editor's job, and
 * this is where it is done.
 */
class PreviewHold {
  html        ;
  stale         ;

  constructor() {
    this.html = "";
    this.stale = false;
  }

  update(response                 )                                   {
    const failed = response.diagnostics.some((diagnostic) => diagnostic.severity === "error");
    if (!failed) {
      this.html = response.html;
      this.stale = false;
    } else {
      this.stale = this.html !== "";
    }
    return { html: this.html, stale: this.stale };
  }
}

// The editor application.
//
// This is the only module that touches the document, the network, storage or
// the WebAssembly module. Everything else is a function from state to markup
// or to `SegmentEdit`s, which is why `test/` needs no browser: a test against a
// stand-in DOM proves the stand-in works. `web/dashboard/src/dashboard.ts`
// took the same shape, for the same reason.
//
// Nothing here runs under `node --test`. What is testable about the shell is
// the markup its renderers produce, and those are pure and live beside the
// state they render.





















/** What the shell holds while a draft is open. */
                 
                            
                    
               
                            
                 
               
                  
                   
                       
 

const state        = {
  mode: "visual",
  advanced: false,
  grant: { role: "contributor" },
  model: null,
  source: "",
  path: "",
  version: 0,
  baseText: "",
  announcement: "",
};

// --- rendering --------------------------------------------------------------

/** One block of the visual mode, as the markup the surface shows. */
function renderBlock(block               )           {
  const body = block.text.replace(/\n+$/, "");
  return html`<div class="block block-${block.kind}" data-block="${block.id}" tabindex="0" role="group"
    aria-label="${block.kind}">${body}</div>`;
}

/** The whole visual surface. */
function renderSurface(model             )           {
  const parts             = [];
  for (const node of model.nodes) {
    if (node.blocks) {
      for (const block of node.blocks) parts.push(renderBlock(block));
      continue;
    }
    parts.push(
      html`<div class="block block-${node.kind}" data-block="${node.id}" tabindex="0" role="group"
        aria-label="${node.name ?? node.kind}">${node.text.replace(/\n+$/, "")}</div>`,
    );
  }
  return html`<div class="surface" data-editor-surface>${parts}</div>`;
}

/**
 * Where a code's help page may be linked from.
 *
 * A `Diagnostic`'s `url` is a string in the payload — generated from the code
 * on the Rust side, but a *value* by the time it reaches here, and this editor
 * renders diagnostics that came over the network. Putting it in an `href`
 * unchecked means a crafted response can run `javascript:` in the editor's own
 * origin, on a link the author has every reason to click.
 */
const HELP_ORIGIN = "https://kasecrab.github.io/";

function helpLink(url        )                {
  return url.startsWith(HELP_ORIGIN) ? url : null;
}

/**
 * The problems pane.
 *
 * Two lines per problem: ED-73's plain-language headline, and the diagnostic's
 * own message underneath when it says something the headline cannot — which
 * field, which prop, which file. The headline alone would tell an author that
 * "a setting has the wrong kind of value" without saying which setting.
 *
 * A code with no plain-language entry — a build-side one reaching this pane
 * through a preview or a verification result — shows its own message as the
 * headline. That is more specific than the registry title the editor used to
 * look up, and it needs nothing derived from `codes.toml` (RFC 2433).
 */
function renderProblems(source        , diagnostics                                        )           {
  const placed = placeDiagnostics(source, diagnostics);
  if (placed.length === 0) return html`<p class="empty">No problems found.</p>`;
  return html`<ul class="problems">
    ${placed.map((entry) => {
      const shown = shownFor(entry.diagnostic);
      const link = helpLink(entry.diagnostic.url);
      return html`<li class="problem problem-${entry.diagnostic.severity}">
        <span class="where">Line ${entry.from.line}</span>
        <span class="what">
          ${shown.headline}
          ${shown.detail === null ? null : html`<span class="detail">${shown.detail}</span>`}
        </span>
        ${shown.fix ? html`<button type="button" data-fix="${shown.fix.action}">${shown.fix.label}</button>` : null}
        ${link
          ? html`<a class="code" href="${link}">${entry.diagnostic.code}</a>`
          : html`<span class="code">${entry.diagnostic.code}</span>`}
      </li>`;
    })}
  </ul>`;
}

/**
 * The toolbar.
 *
 * Its main button is one this person can press (ED-75), and the words are
 * ED-72's: `say()` adds the git term only when the advanced disclosure is on.
 */
function renderToolbar(grant       , mode                     , advanced         )           {
  const primary = primaryAction(grant);
  return html`<header class="toolbar" role="banner" aria-label="Editor toolbar">
    <button type="button" data-mode-switch aria-keyshortcuts="Control+E">
      ${mode === "visual" ? "Source" : "Visual"}
    </button>
    <button type="button" data-action="${primary.action}" class="primary">${primary.label}</button>
    <button type="button" data-help aria-keyshortcuts="?">Help</button>
    <label class="advanced">
      <input type="checkbox" data-advanced ${advanced ? raw("checked") : null} />
      Show ${say("draft", advanced)} details
    </label>
  </header>`;
}

/** The keyboard-shortcut sheet, which is also ED-80's evidence. */
function renderShortcuts()           {
  return html`<table class="shortcuts">
    <caption>Keyboard shortcuts</caption>
    <thead><tr><th scope="col">Keys</th><th scope="col">Does</th><th scope="col">Where</th></tr></thead>
    <tbody>
      ${SHORTCUTS.map(
        (shortcut) => html`<tr><td><kbd>${shortcut.keys}</kbd></td><td>${shortcut.description}</td><td>${shortcut.scope}</td></tr>`,
      )}
    </tbody>
  </table>`;
}

// --- the shell --------------------------------------------------------------

function mount()       {
  const root = document.querySelector("[data-editor]");
  if (!root) return;

  const reduced = globalThis.matchMedia?.("(prefers-reduced-motion: reduce)").matches ?? false;
  root.setAttribute("style", `--motion: ${motionDuration(180, reduced)}ms`);

  for (const landmark of LANDMARKS) {
    const region = root.querySelector(`[data-landmark="${landmark.label}"]`);
    region?.setAttribute("role", landmark.role);
    region?.setAttribute("aria-label", landmark.label);
  }

  announce("Editor ready");
}

function announce(message        )       {
  state.announcement = message;
  const region = document.querySelector("[data-announce]");
  if (region) region.textContent = message;
}

if (typeof document !== "undefined") {
  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", mount, { once: true });
  } else {
    mount();
  }
}

// Every module this application is made of is reachable from the entry, so the
// bundler walks the whole graph and the budget covers all of it. Listing them
// here rather than calling each one is deliberate: the shell wires them up as
// the panes are built, and a bundle that dropped half the editor because the
// shell had not yet reached it would pass its own budget test.
const MODULES = {
  buildModel,
  editBlock,
  editBlocks,
  serializeModel,
  tokenize,
  decorate,
  placeDiagnostics,
  SLASH_COMMANDS,
  slashInsert,
  slashMatches,
  matchShortcut,
  moveBlock,
  htmlToMarkdown,
  imagesIn,
  pasteSource,
  PREVIEW_DEFAULTS,
  previewLimits,
  previewContext,
  capIterations,
  capBytes,
  chipSource,
  parseFrontmatter,
  writeFrontmatter,
  formFields,
  validateFrontmatter,
  routeOf,
  navigationOf,
  createPage,
  renamePage,
  movePage,
  duplicatePage,
  deletePage,
  reorderNavigation,
  findReplace,
  applyPlan,
  changeFactReference,
  moveGroup,
  applyTag,
  sniff,
  validateUpload,
  imageDirective,
  darkPartner,
  searchAssets,
  deleteRefusal,
  transformQuery,
  draftBranch,
  searchDrafts,
  pagesTouched,
  ageOf,
  save,
  merge3,
  seenTab,
  alsoOpenElsewhere,
  call,
  endpointPath,
  findEndpoint,
  unservedBy,
  may,
  refusal,
  primaryAction,
  parseDocowners,
  ownersFor,
  assignReviewers,
  staleReminders,
  queueRows,
  decide,
  publishPlan,
  OPERATIONS,
  runPayload,
  screen,
  setStatus,
  rejectAll,
  acceptedEdits,
  pendingCount,
  ACTIVITY_KINDS,
  KIND_LABEL,
  feed,
  byDay,
  counts,
  TASKS,
  suggestEditUrl,
  quickFixProposal,
  updateANumber,
  renameEverywhere,
  replaceAScreenshot,
  addAFaqEntry,
  recordAChangelogEntry,
  VOCABULARY,
  say,
  TOUR,
  TEMPLATES,
  pageFromTemplate,
  HELP,
  chartTable,
  saveAnnouncement,
  validationAnnouncement,
  reviewAnnouncement,
  messageFor,
  EditorSession,
  PreviewHold,
  sessionNonce,
  renderSurface,
  renderBlock,
  renderProblems,
  renderToolbar,
  renderShortcuts,
  announce,
  state,
};
})();
