//! Expansion, rewrite, and parse contracts (PRD §7.3.1, §7.5.1, §34.9).
//!
//! The entry points themselves (`scan`, `expand`, `rewrite`, `parse`,
//! `render_html`, `render_markdown`, `serialize_source`, `format`) live in
//! `liyasa-markdown`; the types they exchange are frozen here.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::document::{Origin, Props};
use crate::ids::{FactId, Locale, Version};
use crate::net::Url;
use crate::span::{SourceId, Span};

/// Expanded text plus the two maps that lead any position in it back to source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Expanded {
    pub text: String,
    pub map: SpanMap,
    pub record: ExpansionRecord,
}

/// Sorted, non-overlapping `(start, end, origin)` runs over the expanded text.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SpanMap(pub Vec<(u32, u32, Origin)>);

impl SpanMap {
    /// The origin of one expanded byte, by binary search.
    pub fn origin_at(&self, offset: u32) -> Option<&Origin> {
        let at = self.0.partition_point(|(start, _, _)| *start <= offset);
        self.0
            .get(at.checked_sub(1)?)
            .filter(|(_, end, _)| offset < *end)
            .map(|(_, _, origin)| origin)
    }

    /// Every run overlapping an expanded range, in order.
    pub fn origins_in(&self, start: u32, end: u32) -> impl Iterator<Item = &(u32, u32, Origin)> {
        let first = self.0.partition_point(|(_, run_end, _)| *run_end <= start);
        self.0[first..]
            .iter()
            .take_while(move |(run_start, _, _)| *run_start < end)
    }

    /// Whether the runs are sorted and disjoint, which `origin_at` assumes.
    pub fn is_well_formed(&self) -> bool {
        self.0.windows(2).all(|pair| pair[0].1 <= pair[1].0)
            && self.0.iter().all(|(start, end, _)| start <= end)
    }
}

/// What a page read during expansion. Drives variant discovery (§6.6.3) and the
/// dependency graph.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ExpansionRecord {
    pub facts: BTreeSet<FactId>,
    pub includes: Vec<SourceId>,
    /// `reader.<field>` names the page read.
    pub reader_fields: BTreeSet<String>,
    pub dimensions: BTreeSet<String>,
    /// Allow-listed environment variables read through `env()`.
    pub env: BTreeSet<String>,
}

/// Values a page is expanded against.
#[derive(Debug, Clone)]
pub struct TemplateContext {
    pub values: minijinja::Value,
    /// Whether `reader.*` and dimension reads are recorded (§6.6.3 item 1).
    pub tracking: bool,
}

/// Per-line byte deltas introduced by the marker rewrite, which changes line
/// lengths but never line numbers (§7.5.1 item 2).
///
/// Each entry is `(rewritten line start, delta)` where the delta is
/// **cumulative over the lines before it**: for any offset on that line,
/// `expanded = rewritten - delta`. A line's own change in length therefore
/// applies from the next entry on, never to its own bytes. Entries are sorted
/// by line start, and a line the rewrite left alone needs no entry.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RewriteMap(pub Vec<(u32, i32)>);

impl RewriteMap {
    /// A rewritten byte offset back to its expanded offset.
    ///
    /// Positions inside a marker line are not byte-addressable in the expanded
    /// text — the marker replaced the directive — so they clamp to the start of
    /// the line, and the caller resolves them through the directive's recorded
    /// prop spans instead.
    pub fn to_expanded(&self, rewritten: u32) -> u32 {
        let at = self
            .0
            .partition_point(|(line_start, _)| *line_start <= rewritten);
        let Some((line_start, delta)) = self.0.get(at.saturating_sub(1)) else {
            return rewritten;
        };
        let expanded_line_start = line_start.saturating_add_signed(-delta);
        expanded_line_start + (rewritten - line_start)
    }
}

/// Directive names, props, and spans held out of band so no author-controlled
/// text ever enters a marker comment (§7.5.1 item 2).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DirectiveTable(pub Vec<DirectiveInfo>);

impl DirectiveTable {
    /// Marker IDs index the table directly.
    pub fn get(&self, id: usize) -> Option<&DirectiveInfo> {
        self.0.get(id)
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct DirectiveInfo {
    pub name: String,
    pub props: Props,
    pub kind: ComponentKind,
    /// In expanded coordinates.
    pub span: Span,
    pub prop_spans: Vec<(String, Span)>,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "lowercase")]
pub enum ComponentKind {
    Container,
    Leaf,
    Inline,
}

#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "lowercase")]
pub enum HtmlMode {
    Allow,
    #[default]
    Sanitize,
    Off,
}

#[derive(Debug, Clone)]
pub struct ParseOptions {
    pub html: HtmlMode,
    pub math: bool,
    pub wikilinks: bool,
    /// 128 bits generated per build and never written to output; what makes a
    /// marker unforgeable (§7.5.1 item 2).
    pub build_nonce: [u8; 16],
}

impl Default for ParseOptions {
    fn default() -> Self {
        Self {
            html: HtmlMode::Sanitize,
            math: true,
            wikilinks: false,
            build_nonce: [0; 16],
        }
    }
}

/// Who a Markdown serialization is for (§11.7).
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "lowercase")]
pub enum Audience {
    #[default]
    Human,
    Agent,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SiteMeta {
    pub name: String,
    pub canonical_origin: Url,
    pub llms_txt: Url,
    pub version: Option<Version>,
    pub locale: Locale,
}
