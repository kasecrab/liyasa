//! The two page representations (PRD §7.16, §34.9).
//!
//! The Source Document is a lossless segmentation of the file's bytes for the
//! editor, formatter, and LSP. The Rendered AST is what expansion and parsing
//! produce. §34.9 is authoritative for these names; `contract-lint` fails CI
//! when the prose in §7.16 and these declarations disagree.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::diagnostics::Diagnostics;
use crate::frontmatter::Frontmatter;
use crate::ids::{BlockId, FactId, PageId, Route};
use crate::span::{SourceId, Span};

// ---- Source Document ----

/// A lossless segmentation of one file: concatenating the segment spans
/// reproduces the bytes exactly (asserted by a property test).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SourceDocument {
    pub source: SourceId,
    pub frontmatter: Option<Frontmatter>,
    pub segments: Vec<Segment>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "segment", rename_all = "camelCase")]
pub enum Segment {
    Markdown {
        span: Span,
    },
    Code {
        span: Span,
        info: FenceInfo,
        /// The fence body without the delimiter lines.
        body: Span,
    },
    Template {
        span: Span,
        kind: TemplateKind,
    },
    DirectiveOpen {
        span: Span,
        name: String,
        props: Props,
        colons: u8,
        /// Index of the matching [`Segment::DirectiveClose`], absent when the
        /// container is unclosed (`E0310`).
        matching: Option<usize>,
    },
    DirectiveClose {
        span: Span,
    },
    DirectiveLeaf {
        span: Span,
        name: String,
        props: Props,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum TemplateKind {
    /// `{{ … }}`
    Output,
    /// `{% … %}`
    Statement {
        name: String,
        /// Index of the segment that closes this block statement; `None` for a
        /// statement that does not open a block.
        matching: Option<usize>,
    },
    /// `{# … #}`
    Comment,
}

impl Segment {
    pub fn span(&self) -> Span {
        match self {
            Self::Markdown { span }
            | Self::Code { span, .. }
            | Self::Template { span, .. }
            | Self::DirectiveOpen { span, .. }
            | Self::DirectiveClose { span }
            | Self::DirectiveLeaf { span, .. } => *span,
        }
    }
}

/// One replacement applied by `serialize_source`, which is byte-preserving
/// everywhere else.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SegmentEdit {
    pub segment: usize,
    pub new_text: String,
}

// ---- props and fences ----

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(transparent)]
pub struct Props(pub BTreeMap<String, PropValue>);

impl Props {
    pub fn get(&self, name: &str) -> Option<&PropValue> {
        self.0.get(name)
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "type", content = "value", rename_all = "camelCase")]
pub enum PropValue {
    Str(String),
    Num(f64),
    Bool(bool),
    List(Vec<PropValue>),
    /// `{{ … }}`, kept unevaluated in a Source Document and resolved during
    /// expansion.
    Expr(String),
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct FenceInfo {
    pub lang: Option<String>,
    pub attrs: FenceAttrs,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct FenceAttrs {
    pub flags: BTreeSet<String>,
    pub kv: BTreeMap<String, String>,
    /// Inclusive 1-based line ranges to highlight.
    pub highlight: Vec<(u32, u32)>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(transparent)]
pub struct Slots(pub BTreeMap<String, Vec<Node>>);

// ---- origins ----

/// One link in the chain from a page down to the construct that emitted a byte,
/// modelled on a Rust macro backtrace (§7.3.1 item 5).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "frame", rename_all = "camelCase")]
pub enum Frame {
    Include { file: SourceId, at: Span },
    Snippet { name: String, at: Span },
    Macro { name: String, at: Span },
    Loop { at: Span, index: u32 },
    Generated { by: Span },
}

/// Where a node came from. `span` is `None` for content a template emitted;
/// `frames` is innermost-last.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Origin {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span: Option<Span>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub frames: Vec<Frame>,
}

impl Origin {
    pub fn at(span: Span) -> Self {
        Self {
            span: Some(span),
            frames: Vec::new(),
        }
    }

    pub fn generated_by(span: Span) -> Self {
        Self {
            span: None,
            frames: vec![Frame::Generated { by: span }],
        }
    }

    pub fn is_generated(&self) -> bool {
        self.span.is_none()
    }
}

// ---- Rendered AST ----

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Block {
    pub id: BlockId,
    /// The `{#id}` the author attached, if any; `id` is derived from it.
    pub explicit_id: Option<String>,
    pub kind: BlockKind,
    pub origin: Origin,
    pub children: Vec<Node>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "node", content = "value", rename_all = "camelCase")]
pub enum Node {
    Block(Block),
    Inline(Inline),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum BlockKind {
    Document,
    Heading {
        level: u8,
        anchor: String,
    },
    Paragraph,
    List {
        ordered: bool,
        start: u32,
        tight: bool,
    },
    ListItem {
        checked: Option<bool>,
    },
    BlockQuote,
    CodeBlock {
        lang: Option<String>,
        attrs: FenceAttrs,
        highlighted: Option<String>,
    },
    HtmlBlock {
        html: String,
    },
    Table {
        align: Vec<Align>,
    },
    TableRow {
        header: bool,
    },
    TableCell,
    ThematicBreak,
    FootnoteDefinition {
        label: String,
    },
    Math {
        display: bool,
        src: String,
    },
    Component {
        name: String,
        props: Props,
        slots: Slots,
    },
    /// Emitted in `dev` only, for the error overlay.
    LogicMarker {
        text: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", content = "value", rename_all = "camelCase")]
pub enum Inline {
    Text(String),
    Emph(Vec<Inline>),
    Strong(Vec<Inline>),
    Strike(Vec<Inline>),
    Code(String),
    Link {
        href: String,
        title: Option<String>,
        children: Vec<Inline>,
        resolved: Option<Route>,
    },
    Image {
        src: String,
        alt: String,
        title: Option<String>,
        dark: Option<String>,
    },
    HtmlInline(String),
    FootnoteRef(String),
    SoftBreak,
    HardBreak,
    InlineComponent {
        name: String,
        props: Props,
        children: Vec<Inline>,
    },
    Math(String),
    /// Source Document fragments only: a `{{ … }}` left unexpanded for the
    /// visual editor.
    TemplateInline {
        expr: String,
        origin: Span,
    },
}

#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "lowercase")]
pub enum Align {
    #[default]
    None,
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Document {
    pub root: Block,
    pub deps: Deps,
    pub diagnostics: Diagnostics,
}

// ---- dependency edges (§14.12) ----

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(transparent)]
pub struct Deps(pub Vec<Edge>);

/// One edge of the truth graph.
///
/// §34.9 names this shape twice, `Dep` in the supporting types and `Edge` in
/// the truth-engine decomposition; they are one type under two names so that
/// `DependencyExtractor::extract` and `Component::deps` cannot drift apart.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Edge {
    pub from: EdgeOrigin,
    pub to: DepTarget,
    pub kind: EdgeKind,
}

pub type Dep = Edge;

/// Page identity is a ULID, so an edge survives a rename (§7.16).
#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(tag = "origin", content = "value", rename_all = "camelCase")]
pub enum EdgeOrigin {
    Block(PageId, BlockId),
    Page(PageId),
}

#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(tag = "target", content = "value", rename_all = "camelCase")]
#[non_exhaustive]
pub enum DepTarget {
    Fact(FactId),
    Source(String),
    Snippet(SourceId),
    Component(String),
    Asset(String),
    Operation { spec: String, op: String },
    ExternalUrl(String),
    Screenshot(String),
    Page(Route),
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum EdgeKind {
    Reads,
    Includes,
    Links,
    Embeds,
    Documents,
}
