//! The shapes `web/editor` and the search worker program against (PRD §6.11).
//!
//! Frozen: a field that changes here changes `ts/liyasa-wasm.d.ts`, which is
//! checked in and compared against these types by
//! `tests/it/typescript.rs`, so the change is visible in a diff rather than
//! discovered by the editor at run time.
//!
//! Field names are snake_case, matching the `liyasa-core` types the payloads
//! embed (`SourceDocument`, `Document`, `Diagnostic`). One convention across
//! the whole payload beats a camelCase envelope around snake_case contents.

use liyasa_core::diagnostics::Diagnostic;
use liyasa_core::document::{Document, SegmentEdit, SourceDocument};
use liyasa_core::markdown::{ExpansionRecord, HtmlMode};
use serde::{Deserialize, Serialize};

/// What a session needs once, before the first keystroke.
#[derive(Debug, Clone, Default, Serialize, Deserialize, schemars::JsonSchema)]
pub struct OpenRequest {
    /// 32 hexadecimal characters, generated per session by the host that owns
    /// the draft. It is what makes a directive marker unforgeable (§7.5.1
    /// item 2): with a known nonce an author can type a marker into the page
    /// and have the preview parse it as a component nobody declared. There is
    /// no default for that reason.
    pub nonce: String,
    #[serde(default)]
    pub site: SiteMeta,
    /// ED-07's preload set: the open page, its snippets, components, vars and
    /// facts, recorded in the page's `ExpansionRecord` and sent in one request
    /// when the page opens.
    #[serde(default)]
    pub seed: Vec<SeedEntry>,
}

/// One preloaded file. `bytes` is the file as the draft holds it.
#[derive(Debug, Clone, Default, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SeedEntry {
    pub path: String,
    pub text: String,
}

/// The site metadata a Markdown serialization needs (§11.7). The editor knows
/// all of it from the draft's project; none of it is guessed here.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SiteMeta {
    pub name: String,
    pub canonical_origin: String,
    pub llms_txt: String,
    #[serde(default)]
    pub version: Option<String>,
    pub locale: String,
}

impl Default for SiteMeta {
    fn default() -> Self {
        Self {
            name: String::new(),
            canonical_origin: "https://example.invalid".to_owned(),
            llms_txt: "https://example.invalid/llms.txt".to_owned(),
            version: None,
            locale: "en".to_owned(),
        }
    }
}

/// How a draft is parsed. The build's own defaults, minus the nonce, which
/// belongs to the session rather than to one call.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Options {
    #[serde(default)]
    pub html: HtmlMode,
    #[serde(default = "enabled")]
    pub math: bool,
    #[serde(default)]
    pub wikilinks: bool,
}

fn enabled() -> bool {
    true
}

impl Default for Options {
    fn default() -> Self {
        Self {
            html: HtmlMode::default(),
            math: true,
            wikilinks: false,
        }
    }
}

/// A draft to segment and parse.
#[derive(Debug, Clone, Default, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ParseRequest {
    /// The draft's path, so a diagnostic points at the file the author opened.
    pub path: String,
    pub source: String,
    /// The values `{{ ... }}` expands against.
    #[serde(default)]
    pub context: serde_json::Value,
    #[serde(default)]
    pub options: Options,
}

/// Both representations of §7.16: the lossless segmentation the source mode
/// edits, and the Rendered AST the visual mode edits.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ParseResponse {
    pub source: SourceDocument,
    /// `None` when expansion failed; `diagnostics` says why.
    pub document: Option<Document>,
    pub record: ExpansionRecord,
    /// ED-07: paths this draft needs that the session does not hold. The host
    /// fetches each through `/_liyasa/editor/fs/<path>`, hands it back with
    /// `Session.seed`, and calls again.
    pub missing: Vec<String>,
    pub diagnostics: Vec<Diagnostic>,
}

/// A draft to render (the keystroke path, NFR-05).
#[derive(Debug, Clone, Default, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PreviewRequest {
    pub path: String,
    pub source: String,
    #[serde(default)]
    pub context: serde_json::Value,
    #[serde(default)]
    pub options: Options,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PreviewResponse {
    pub html: String,
    /// The same render as `html`, serialized for an agent (§11.7), so the two
    /// can never come from different parses.
    pub markdown: String,
    pub text: String,
    pub record: ExpansionRecord,
    /// ED-07: paths this draft needs that the session does not hold. The host
    /// fetches each through `/_liyasa/editor/fs/<path>`, hands it back with
    /// `Session.seed`, and calls again.
    pub missing: Vec<String>,
    /// ED-07: the preload set is over budget, nothing was rendered, and the
    /// editor shows the large-page notice and asks the preview endpoint
    /// instead. `diagnostics` carries `W1201`.
    pub server_render: bool,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ValidateMode {
    #[default]
    Dev,
    Build,
}

/// What the editor asks about a draft that is not Markdown: `liyasa.json` and
/// the page's front matter.
#[derive(Debug, Clone, Default, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ValidateRequest {
    /// `liyasa.json` as the editor holds it.
    #[serde(default)]
    pub config: Option<String>,
    /// A page's front matter, YAML, without the `---` fences.
    #[serde(default)]
    pub frontmatter: Option<String>,
    #[serde(default)]
    pub mode: ValidateMode,
    /// The routes navigation may reference. Empty means the editor does not
    /// know them yet, and the navigation checks are skipped rather than
    /// reporting every entry as missing.
    #[serde(default)]
    pub routes: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ValidateResponse {
    /// The config as parsed, so the editor can show defaults it did not write.
    /// `None` when it is not valid JSON.
    pub config: Option<serde_json::Value>,
    pub diagnostics: Vec<Diagnostic>,
}

/// Segment edits to write back. Byte-preserving everywhere the caller did not
/// edit, which is the whole point of the Source Document (§34.9).
#[derive(Debug, Clone, Default, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SerializeRequest {
    pub path: String,
    pub source: String,
    #[serde(default)]
    pub edits: Vec<SegmentEdit>,
    /// Run the canonical formatter over the result (CLI-05). Off by default:
    /// an editor that reformats a page the author did not touch is the failure
    /// the Source Document exists to prevent.
    #[serde(default)]
    pub format: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SerializeResponse {
    pub text: String,
    pub diagnostics: Vec<Diagnostic>,
}

/// One query from the search dialog (§12.2, SRC-05).
#[derive(Debug, Clone, Default, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SearchRequest {
    pub query: String,
    /// The locale, version and tab of the page the dialog was opened on.
    #[serde(default)]
    pub locale: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub tab: Option<String>,
    #[serde(default)]
    pub max_results: Option<u32>,
    #[serde(default)]
    pub snippets: Option<bool>,
    /// The reader's own scope, which decides what they may see.
    #[serde(default)]
    pub reader_groups: Vec<String>,
    #[serde(default)]
    pub reader_region: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SearchResponse {
    pub hits: Vec<SearchHit>,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SearchHit {
    pub url: String,
    pub route: String,
    pub anchor: String,
    pub title: String,
    pub section: String,
    pub breadcrumb: Vec<String>,
    /// `page`, `endpoint` or `changelog` (RX-32).
    pub kind: String,
    pub tab: Option<String>,
    pub version: Option<String>,
    pub locale: Option<String>,
    pub score: f32,
    /// How many of the query's terms this document matched.
    pub matched: u32,
    pub snippet: Option<Snippet>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Snippet {
    pub text: String,
    /// `[start, end)` byte ranges within `text`, ascending and non-overlapping.
    pub highlights: Vec<Highlight>,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Highlight {
    pub start: u32,
    pub end: u32,
}

/// Whether the session may preview in the browser at all (ED-07).
#[derive(Debug, Clone, Default, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SessionStatus {
    /// Bytes the seed holds.
    pub preload_bytes: u32,
    /// ED-07's 2 MB cap.
    pub preload_limit: u32,
    /// How many paths the session resolved through the server since it opened.
    pub fetched: u32,
    /// The seed is over the cap: previews are served by the preview endpoint
    /// and the editor shows the large-page notice.
    pub server_render: bool,
}
