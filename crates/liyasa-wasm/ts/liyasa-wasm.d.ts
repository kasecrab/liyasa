// Generated from `crates/liyasa-wasm/src/api.rs` by `liyasa_wasm::ts`.
// Do not edit: `cargo test -p liyasa-wasm typescript` rewrites it and fails
// when this file and the Rust types disagree.

export interface Highlight {
  end: number;
  start: number;
}

/**
 * What a session needs once, before the first keystroke.
 */
export interface OpenRequest {
  /**
   * 32 hexadecimal characters, generated per session by the host that owns
   * the draft. It is what makes a directive marker unforgeable (§7.5.1
   * item 2): with a known nonce an author can type a marker into the page
   * and have the preview parse it as a component nobody declared. There is
   * no default for that reason.
   */
  nonce: string;
  /**
   * ED-07's preload set: the open page, its snippets, components, vars and
   * facts, recorded in the page's `ExpansionRecord` and sent in one request
   * when the page opens.
   */
  seed?: SeedEntry[];
  site?: SiteMeta;
}

/**
 * How a draft is parsed. The build's own defaults, minus the nonce, which
 * belongs to the session rather than to one call.
 */
export interface Options {
  html?: HtmlMode;
  math?: boolean;
  wikilinks?: boolean;
}

/**
 * A draft to segment and parse.
 */
export interface ParseRequest {
  /**
   * The values `{{ ... }}` expands against.
   */
  context?: unknown;
  options?: Options;
  /**
   * The draft's path, so a diagnostic points at the file the author opened.
   */
  path: string;
  source: string;
}

/**
 * Both representations of §7.16: the lossless segmentation the source mode
 * edits, and the Rendered AST the visual mode edits.
 */
export interface ParseResponse {
  diagnostics: Diagnostic[];
  /**
   * `None` when expansion failed; `diagnostics` says why.
   */
  document?: Document | null;
  /**
   * ED-07: paths this draft needs that the session does not hold. The host
   * fetches each through `/_liyasa/editor/fs/<path>`, hands it back with
   * `Session.seed`, and calls again.
   */
  missing: string[];
  record: ExpansionRecord;
  source: SourceDocument;
}

/**
 * A draft to render (the keystroke path, NFR-05).
 */
export interface PreviewRequest {
  context?: unknown;
  options?: Options;
  path: string;
  source: string;
}

export interface PreviewResponse {
  diagnostics: Diagnostic[];
  html: string;
  /**
   * The same render as `html`, serialized for an agent (§11.7), so the two
   * can never come from different parses.
   */
  markdown: string;
  /**
   * ED-07: paths this draft needs that the session does not hold. The host
   * fetches each through `/_liyasa/editor/fs/<path>`, hands it back with
   * `Session.seed`, and calls again.
   */
  missing: string[];
  record: ExpansionRecord;
  /**
   * ED-07: the preload set is over budget, nothing was rendered, and the
   * editor shows the large-page notice and asks the preview endpoint
   * instead. `diagnostics` carries `W1201`.
   */
  server_render: boolean;
  text: string;
}

export interface SearchHit {
  anchor: string;
  breadcrumb: string[];
  /**
   * `page`, `endpoint` or `changelog` (RX-32).
   */
  kind: string;
  locale?: string | null;
  /**
   * How many of the query's terms this document matched.
   */
  matched: number;
  route: string;
  score: number;
  section: string;
  snippet?: Snippet | null;
  tab?: string | null;
  title: string;
  url: string;
  version?: string | null;
}

/**
 * One query from the search dialog (§12.2, SRC-05).
 */
export interface SearchRequest {
  /**
   * The locale, version and tab of the page the dialog was opened on.
   */
  locale?: string | null;
  max_results?: number | null;
  query: string;
  /**
   * The reader's own scope, which decides what they may see.
   */
  reader_groups?: string[];
  reader_region?: string | null;
  snippets?: boolean | null;
  tab?: string | null;
  version?: string | null;
}

export interface SearchResponse {
  diagnostics: Diagnostic[];
  hits: SearchHit[];
}

/**
 * One preloaded file. `bytes` is the file as the draft holds it.
 */
export interface SeedEntry {
  path: string;
  text: string;
}

/**
 * Segment edits to write back. Byte-preserving everywhere the caller did not
 * edit, which is the whole point of the Source Document (§34.9).
 */
export interface SerializeRequest {
  edits?: SegmentEdit[];
  /**
   * Run the canonical formatter over the result (CLI-05). Off by default:
   * an editor that reformats a page the author did not touch is the failure
   * the Source Document exists to prevent.
   */
  format?: boolean;
  path: string;
  source: string;
}

export interface SerializeResponse {
  diagnostics: Diagnostic[];
  text: string;
}

/**
 * Whether the session may preview in the browser at all (ED-07).
 */
export interface SessionStatus {
  /**
   * How many paths the session resolved through the server since it opened.
   */
  fetched: number;
  /**
   * Bytes the seed holds.
   */
  preload_bytes: number;
  /**
   * ED-07's 2 MB cap.
   */
  preload_limit: number;
  /**
   * The seed is over the cap: previews are served by the preview endpoint
   * and the editor shows the large-page notice.
   */
  server_render: boolean;
}

/**
 * The site metadata a Markdown serialization needs (§11.7). The editor knows
 * all of it from the draft's project; none of it is guessed here.
 */
export interface SiteMeta {
  canonical_origin: string;
  llms_txt: string;
  locale: string;
  name: string;
  version?: string | null;
}

export interface Snippet {
  /**
   * `[start, end)` byte ranges within `text`, ascending and non-overlapping.
   */
  highlights: Highlight[];
  text: string;
}

export type ValidateMode = "dev" | "build";

/**
 * What the editor asks about a draft that is not Markdown: `liyasa.json` and
 * the page's front matter.
 */
export interface ValidateRequest {
  /**
   * `liyasa.json` as the editor holds it.
   */
  config?: string | null;
  /**
   * A page's front matter, YAML, without the `---` fences.
   */
  frontmatter?: string | null;
  mode?: ValidateMode;
  /**
   * The routes navigation may reference. Empty means the editor does not
   * know them yet, and the navigation checks are skipped rather than
   * reporting every entry as missing.
   */
  routes?: string[];
}

export interface ValidateResponse {
  /**
   * The config as parsed, so the editor can show defaults it did not write.
   * `None` when it is not valid JSON.
   */
  config?: unknown;
  diagnostics: Diagnostic[];
}


// ---- shared types, from `liyasa-core` ----
/**
 * Whether a page on a private site is readable without authentication
 * (AUTH-07). Distinct from the site-level `public` flag.
 */
export type Access = "inherit" | "public";

export type AiSetting = boolean | {
  instructions?: string | null;
};

export type Align = "none" | "left" | "center" | "right";

export interface Block {
  children: Node[];
  /**
   * The `{#id}` the author attached, if any; `id` is derived from it.
   */
  explicit_id?: string | null;
  id: BlockId;
  kind: BlockKind;
  origin: Origin;
}

export type BlockId = string;

export type BlockKind = {
  kind: "document";
} | {
  anchor: string;
  kind: "heading";
  level: number;
} | {
  kind: "paragraph";
} | {
  kind: "list";
  ordered: boolean;
  start: number;
  tight: boolean;
} | {
  checked?: boolean | null;
  kind: "listItem";
} | {
  kind: "blockQuote";
} | {
  attrs: FenceAttrs;
  highlighted?: string | null;
  kind: "codeBlock";
  lang?: string | null;
} | {
  html: string;
  kind: "htmlBlock";
} | {
  align: Align[];
  kind: "table";
} | {
  header: boolean;
  kind: "tableRow";
} | {
  kind: "tableCell";
} | {
  kind: "thematicBreak";
} | {
  kind: "footnoteDefinition";
  label: string;
} | {
  display: boolean;
  kind: "math";
  src: string;
} | {
  kind: "component";
  name: string;
  props: Record<string, PropValue>;
  slots: Record<string, Node[]>;
} | {
  kind: "logicMarker";
  text: string;
};

/**
 * A code registered in liyasa-core's codes.toml.
 */
export type Code = string;

export type DepTarget = {
  target: "fact";
  value: string;
} | {
  target: "source";
  value: string;
} | {
  target: "snippet";
  value: SourceId;
} | {
  target: "component";
  value: string;
} | {
  target: "asset";
  value: string;
} | {
  target: "operation";
  value: {
    op: string;
    spec: string;
  };
} | {
  target: "externalUrl";
  value: string;
} | {
  target: "screenshot";
  value: string;
} | {
  target: "page";
  value: Route;
};

export interface Diagnostic {
  code: Code;
  help?: string | null;
  labels?: ([Span, string])[];
  message: string;
  related?: Diagnostic[];
  severity: Severity;
  span?: Span | null;
  /**
   * Generated from `code`; carried in the payload so consumers that do not
   * link `liyasa-core` still have it.
   */
  url: string;
}

/**
 * A diagnostic list kept sorted by `(source, start)` (§34.9).
 */
export type Diagnostics = Diagnostic[];

export interface Document {
  deps: Edge[];
  diagnostics: Diagnostics;
  root: Block;
}

/**
 * One edge of the truth graph.
 * 
 * §34.9 names this shape twice, `Dep` in the supporting types and `Edge` in
 * the truth-engine decomposition; they are one type under two names so that
 * `DependencyExtractor::extract` and `Component::deps` cannot drift apart.
 */
export interface Edge {
  from: EdgeOrigin;
  kind: EdgeKind;
  to: DepTarget;
}

export type EdgeKind = "reads" | "includes" | "links" | "embeds" | "documents";

/**
 * Page identity is a ULID, so an edge survives a rename (§7.16).
 */
export type EdgeOrigin = {
  origin: "block";
  value: [PageId, BlockId];
} | {
  origin: "page";
  value: PageId;
};

/**
 * What a page read during expansion. Drives variant discovery (§6.6.3) and the
 * dependency graph.
 */
export interface ExpansionRecord {
  dimensions: string[];
  /**
   * Allow-listed environment variables read through `env()`.
   */
  env: string[];
  facts: string[];
  includes: SourceId[];
  /**
   * `reader.<field>` names the page read.
   */
  reader_fields: string[];
}

export interface FenceAttrs {
  flags: string[];
  /**
   * Inclusive 1-based line ranges to highlight.
   */
  highlight: ([number, number])[];
  kv: Record<string, string>;
}

export interface FenceInfo {
  attrs: FenceAttrs;
  lang?: string | null;
}

/**
 * One link in the chain from a page down to the construct that emitted a byte,
 * modelled on a Rust macro backtrace (§7.3.1 item 5).
 */
export type Frame = {
  at: Span;
  file: SourceId;
  frame: "include";
} | {
  at: Span;
  frame: "snippet";
  name: string;
} | {
  at: Span;
  frame: "macro";
  name: string;
} | {
  at: Span;
  frame: "loop";
  index: number;
} | {
  by: Span;
  frame: "generated";
};

export interface Frontmatter {
  span: Span;
  typed: FrontmatterFields;
  value: unknown;
}

/**
 * The keys of §7.6 that Liyasa itself reads. Every one is optional; `title`
 * falls back to the first H1 or the file name with a warning.
 * 
 * Dates are kept as the author wrote them rather than parsed into a calendar
 * type: the build clock (§6.6.2) is the only time source that reaches output,
 * and no date crate is in the dependency table.
 */
export interface FrontmatterFields {
  access?: Access | null;
  ai?: AiSetting | null;
  asyncapi?: string | null;
  authors?: string[];
  canonical?: string | null;
  date?: string | null;
  description?: string | null;
  draft?: boolean | null;
  facts?: Record<string, unknown>;
  graphql?: string | null;
  groups?: string[];
  hidden?: boolean | null;
  icon?: string | null;
  iconType?: string | null;
  id?: PageId | null;
  keywords?: string[];
  locales?: Locale[];
  mode?: PageMode | null;
  noindex?: boolean | null;
  og?: SocialMeta | null;
  /**
   * `"spec-id METHOD /path"`.
   */
  openapi?: string | null;
  /**
   * Declares that the page reads free-form `reader.*` fields and is rendered
   * on demand (§6.6.4); without it, `reader.*` is `E0208`.
   */
  personalized?: boolean | null;
  product?: string | null;
  regions?: RegionGate | null;
  /**
   * Page IDs or routes for the related topics block.
   */
  related?: string[];
  reviewed?: string | null;
  search?: SearchSetting | null;
  sidebarTitle?: string | null;
  slug?: string | null;
  tag?: string | null;
  template?: string | null;
  title?: string | null;
  twitter?: SocialMeta | null;
  updated?: string | null;
  /**
   * External URL; the page is a navigation link only and has no body.
   */
  url?: string | null;
  variation?: string[];
  /**
   * Mirrors the `verify` schema object (§14).
   */
  verify?: unknown;
  versions?: Version[];
}

export type HtmlMode = "allow" | "sanitize" | "off";

export type Inline = {
  kind: "text";
  value: string;
} | {
  kind: "emph";
  value: Inline[];
} | {
  kind: "strong";
  value: Inline[];
} | {
  kind: "strike";
  value: Inline[];
} | {
  kind: "code";
  value: string;
} | {
  kind: "link";
  value: {
    children: Inline[];
    href: string;
    resolved?: Route | null;
    title?: string | null;
  };
} | {
  kind: "image";
  value: {
    alt: string;
    dark?: string | null;
    src: string;
    title?: string | null;
  };
} | {
  kind: "htmlInline";
  value: string;
} | {
  kind: "footnoteRef";
  value: string;
} | {
  kind: "softBreak";
} | {
  kind: "hardBreak";
} | {
  kind: "inlineComponent";
  value: {
    children: Inline[];
    name: string;
    props: Record<string, PropValue>;
  };
} | {
  kind: "math";
  value: string;
} | {
  kind: "templateInline";
  value: {
    expr: string;
    origin: Span;
  };
};

/**
 * A BCP 47 language tag such as `en` or `pt-BR`.
 */
export type Locale = string;

export type Node = {
  node: "block";
  value: Block;
} | {
  node: "inline";
  value: Inline;
};

/**
 * Where a node came from. `span` is `None` for content a template emitted;
 * `frames` is innermost-last.
 */
export interface Origin {
  frames?: Frame[];
  span?: Span | null;
}

export type PageId = string;

/**
 * Page layout (§7.7).
 */
export type PageMode = "default" | "wide" | "custom" | "frame" | "center" | "assistant";

export type PropValue = {
  type: "str";
  value: string;
} | {
  type: "num";
  value: number;
} | {
  type: "bool";
  value: boolean;
} | {
  type: "list";
  value: PropValue[];
} | {
  type: "expr";
  value: string;
};

export interface RegionGate {
  except?: string[] | null;
  only?: string[] | null;
}

/**
 * A site-relative route with a leading slash and no trailing slash, such
 * as `/getting-started/install`.
 */
export type Route = string;

export type SearchSetting = boolean | {
  boost?: number | null;
  exclude?: boolean | null;
};

export type Segment = {
  segment: "markdown";
  span: Span;
} | {
  /**
   * The fence body without the delimiter lines.
   */
  body: Span;
  info: FenceInfo;
  segment: "code";
  span: Span;
} | {
  kind: TemplateKind;
  segment: "template";
  span: Span;
} | {
  colons: number;
  /**
   * Index of the matching [`Segment::DirectiveClose`], absent when the
   * container is unclosed (`E0310`).
   */
  matching?: number | null;
  name: string;
  props: Record<string, PropValue>;
  segment: "directiveOpen";
  span: Span;
} | {
  segment: "directiveClose";
  span: Span;
} | {
  name: string;
  props: Record<string, PropValue>;
  segment: "directiveLeaf";
  span: Span;
};

/**
 * One replacement applied by `serialize_source`, which is byte-preserving
 * everywhere else.
 */
export interface SegmentEdit {
  new_text: string;
  segment: number;
}

export type Severity = "error" | "warning" | "info" | "hint";

export interface SocialMeta {
  description?: string | null;
  image?: string | null;
  title?: string | null;
}

/**
 * A lossless segmentation of one file: concatenating the segment spans
 * reproduces the bytes exactly (asserted by a property test).
 */
export interface SourceDocument {
  frontmatter?: Frontmatter | null;
  segments: Segment[];
  source: SourceId;
}

/**
 * A path interned into a [`SourceMap`](crate::source_map::SourceMap) for the
 * life of one build.
 * 
 * Diagnostics are serialized with the path rather than this index so they
 * survive crate and process boundaries (§34.9).
 */
export type SourceId = number;

/**
 * A half-open byte range within one source.
 */
export interface Span {
  end: number;
  source: SourceId;
  start: number;
}

export type TemplateKind = {
  kind: "output";
} | {
  kind: "statement";
  /**
   * Index of the segment that closes this block statement; `None` for a
   * statement that does not open a block.
   */
  matching?: number | null;
  name: string;
} | {
  kind: "comment";
};

/**
 * A documentation version label such as `v2`, not a semver of Liyasa.
 */
export type Version = string;


// ---- the objects ----

/** One editor session over one draft (ED-07). */
export declare class Session {
  static open(request: OpenRequest): Session;
  free(): void;
  /**
   * Hands over a path a response named in `missing`. The host fetched it
   * through `/_liyasa/editor/fs/<path>`; the module does no I/O of its own.
   */
  seed(path: string, text: string): void;
  parse(request: ParseRequest): ParseResponse;
  preview(request: PreviewRequest): PreviewResponse;
  validate(request: ValidateRequest): ValidateResponse;
  serialize(request: SerializeRequest): SerializeResponse;
  status(): SessionStatus;
}

/** The browser search worker over `liyasa-idx` shard bytes (SRC-05). */
export declare class Searcher {
  /** The bytes of `manifest.json`, which the worker fetches first. */
  static open(manifest: Uint8Array): Searcher;
  free(): void;
  addFile(name: string, bytes: Uint8Array): void;
  search(request: SearchRequest): SearchResponse;
}
