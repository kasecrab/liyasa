//! Page size and what a page's size is made of (CM-144, CM-145; spec checks
//! `page-size-markdown` and `embedded-data-serialization`).
//!
//! An agent reads a page in one fetch and a pipeline truncates what does not
//! fit. Two things follow: a page has a size ceiling, and when a page is near
//! it, the reader deserves to know whether the bytes are prose or a generated
//! table nobody meant to publish whole.

use std::fmt::Write as _;

use liyasa_components::registry::Registry;
use liyasa_components::render::{MarkdownCtx, Shared};
use liyasa_core::diagnostics::{Diagnostic, Diagnostics, code};
use liyasa_core::document::{Block, BlockKind, Node};
use liyasa_core::markdown::{Audience, SiteMeta};

/// Over this many characters a page warns (`W0308`).
pub const WARN_CHARS: usize = 50_000;

/// Over this many characters a page fails the build (`E0307`).
pub const ERROR_CHARS: usize = 100_000;

/// A table with more rows than this is machine-generated bulk, not prose.
pub const BULK_TABLE_ROWS: usize = 50;

/// An inline data blob this large is machine-generated bulk.
pub const BULK_BLOB_CHARS: usize = 4 * 1024;

/// The share of a page's converted size at which bulk elements are said to
/// dominate it. The spec names no number; see
/// `plan/rfcs/1002-bulk-element-dominance.md`.
pub const BULK_DOMINANCE: f64 = 0.5;

/// The four fixes CM-145 offers, in the order the requirement lists them.
pub const BULK_FIXES: [&str; 4] = [
    "split the rows across pages",
    "offer a filtered view",
    "load the data on demand",
    "put the prose before the bulk element so the explanation survives truncation",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BulkKind {
    /// A table over [`BULK_TABLE_ROWS`] rows.
    Table { rows: usize },
    /// A fenced block over [`BULK_BLOB_CHARS`].
    DataBlock,
    /// A base64 run over [`BULK_BLOB_CHARS`].
    Base64Run,
}

impl BulkKind {
    fn describe(self) -> String {
        match self {
            Self::Table { rows } => format!("a {rows}-row table"),
            Self::DataBlock => "an inline data block".to_owned(),
            Self::Base64Run => "a base64 run".to_owned(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct BulkElement {
    pub kind: BulkKind,
    /// Characters this element contributes to the converted page.
    pub chars: usize,
    /// The nearest heading above it, so a reader can find it.
    pub under: Option<String>,
}

impl BulkElement {
    pub fn share(&self, total: usize) -> f64 {
        if total == 0 {
            return 0.0;
        }
        self.chars as f64 / total as f64
    }

    fn describe(&self, total: usize) -> String {
        let percent = (self.share(total) * 100.0).round() as u64;
        match &self.under {
            Some(heading) => format!("{} under `{heading}` ({percent}%)", self.kind.describe()),
            None => format!("{} ({percent}%)", self.kind.describe()),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Report {
    pub chars: usize,
    pub bulk: Vec<BulkElement>,
}

impl Report {
    pub fn bulk_chars(&self) -> usize {
        self.bulk.iter().map(|e| e.chars).sum()
    }

    pub fn bulk_share(&self) -> f64 {
        if self.chars == 0 {
            return 0.0;
        }
        self.bulk_chars() as f64 / self.chars as f64
    }

    pub fn is_over_warn(&self) -> bool {
        self.chars > WARN_CHARS
    }

    pub fn is_over_error(&self) -> bool {
        self.chars > ERROR_CHARS
    }
}

/// Measures a page and reports what CM-144 and CM-145 ask for.
pub fn check(
    markdown: &str,
    root: &Block,
    registry: &Registry,
    site: &SiteMeta,
    out: &mut Diagnostics,
) -> Report {
    let report = measure(markdown, root, registry, site);
    if report.is_over_error() {
        out.push(oversized(code::E0307, ERROR_CHARS, markdown, &report));
    } else if report.is_over_warn() {
        out.push(oversized(code::W0308, WARN_CHARS, markdown, &report));
    }
    if report.is_over_warn() && !report.bulk.is_empty() && report.bulk_share() >= BULK_DOMINANCE {
        out.push(bulk_diagnostic(&report));
    }
    report
}

pub fn measure(markdown: &str, root: &Block, registry: &Registry, site: &SiteMeta) -> Report {
    let chars = markdown.chars().count();
    let mut bulk = Vec::new();
    collect(root, registry, site, &mut None, &mut bulk);
    Report { chars, bulk }
}

fn oversized(code: liyasa_core::Code, limit: usize, markdown: &str, report: &Report) -> Diagnostic {
    let mut message = format!(
        "the Markdown of this page is {} characters, over the {limit}-character limit",
        report.chars
    );
    let splits = suggest_splits(markdown, WARN_CHARS);
    if splits.is_empty() {
        message.push_str("; it has no H2 headings to split at");
        return Diagnostic::new(code, message);
    }
    let mut help = String::from("split it at ");
    for (at, heading) in splits.iter().enumerate() {
        if at > 0 {
            let _ = write!(help, ", ");
        }
        let _ = write!(help, "`{heading}`");
    }
    Diagnostic::new(code, message).help(help)
}

fn bulk_diagnostic(report: &Report) -> Diagnostic {
    let percent = (report.bulk_share() * 100.0).round() as u64;
    let named: Vec<String> = report
        .bulk
        .iter()
        .map(|element| element.describe(report.chars))
        .collect();
    Diagnostic::new(
        code::W0321,
        format!(
            "machine-generated bulk is {percent}% of this page's converted size: {}",
            named.join(", ")
        ),
    )
    .help(BULK_FIXES.join("; "))
}

/// The H2 headings a page can be cut at, chosen so no piece exceeds `limit`.
pub fn suggest_splits(markdown: &str, limit: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut since_cut = 0usize;
    let mut in_fence: Option<String> = None;
    for line in markdown.lines() {
        let width = line.chars().count() + 1;
        match &in_fence {
            Some(fence) if line.trim_start().starts_with(fence.as_str()) => in_fence = None,
            Some(_) => {}
            None => {
                if let Some(fence) = opening_fence(line) {
                    in_fence = Some(fence);
                } else if let Some(heading) = line.strip_prefix("## ")
                    && since_cut + width > limit
                {
                    out.push(heading.trim().to_owned());
                    since_cut = 0;
                }
            }
        }
        since_cut += width;
    }
    out
}

fn opening_fence(line: &str) -> Option<String> {
    let trimmed = line.trim_start();
    let run = trimmed.chars().take_while(|c| *c == '`').count();
    (run >= 3).then(|| "`".repeat(run))
}

/// Walks the tree for elements a machine produced, measuring each by rendering
/// it on its own: an estimate from the source would miss the padding a table
/// gains on the way out.
fn collect(
    block: &Block,
    registry: &Registry,
    site: &SiteMeta,
    heading: &mut Option<String>,
    out: &mut Vec<BulkElement>,
) {
    match &block.kind {
        BlockKind::Heading { level, .. } => {
            let text = liyasa_components::text::of(&block.children);
            *heading = Some(format!("{} {}", "#".repeat((*level).into()), text.trim()));
            return;
        }
        BlockKind::Table { .. } => {
            let rows = block
                .children
                .iter()
                .filter(|child| {
                    matches!(child, Node::Block(row) if matches!(row.kind, BlockKind::TableRow { header: false }))
                })
                .count();
            if rows > BULK_TABLE_ROWS {
                out.push(BulkElement {
                    kind: BulkKind::Table { rows },
                    chars: rendered_chars(block, registry, site),
                    under: heading.clone(),
                });
            }
            return;
        }
        BlockKind::CodeBlock { .. } => {
            let chars = rendered_chars(block, registry, site);
            let body = liyasa_components::text::of(&block.children);
            let kind = if base64_run(&body) >= BULK_BLOB_CHARS {
                Some(BulkKind::Base64Run)
            } else if chars > BULK_BLOB_CHARS {
                Some(BulkKind::DataBlock)
            } else {
                None
            };
            if let Some(kind) = kind {
                out.push(BulkElement {
                    kind,
                    chars,
                    under: heading.clone(),
                });
            }
            return;
        }
        _ => {}
    }
    for child in &block.children {
        match child {
            Node::Block(inner) => collect(inner, registry, site, heading, out),
            Node::Inline(inline) => {
                let text = liyasa_components::text::of(&[Node::Inline(inline.clone())]);
                let run = base64_run(&text);
                if run >= BULK_BLOB_CHARS {
                    out.push(BulkElement {
                        kind: BulkKind::Base64Run,
                        chars: run,
                        under: heading.clone(),
                    });
                }
            }
        }
    }
}

/// How many characters one block contributes to the converted page.
fn rendered_chars(block: &Block, registry: &Registry, site: &SiteMeta) -> usize {
    let reference = liyasa_components::Reference::with(registry);
    // No `.variant`: §6.6.4 calls this surface anonymous — it is written once
    // per page and served to every reader, so it renders under the default
    // variant, which admits no gated block. A populated variant here would put
    // one reader's gated content into a file everyone gets.
    let shared = Shared::new(&reference).site(site).audience(Audience::Agent);
    let mut ctx = MarkdownCtx::with(shared);
    if ctx.children(&[Node::Block(block.clone())]).is_err() {
        return 0;
    }
    ctx.finish().chars().count()
}

/// The longest run of base64 characters in a string.
fn base64_run(text: &str) -> usize {
    let mut longest = 0;
    let mut run = 0;
    for ch in text.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '+' | '/' | '=' | '-' | '_') {
            run += 1;
            longest = longest.max(run);
        } else if !ch.is_whitespace() {
            run = 0;
        }
    }
    longest
}

#[cfg(test)]
mod tests {
    use liyasa_components::nodes;
    use liyasa_core::document::{Align, Block, BlockKind, Node, Origin};
    use liyasa_core::ids::{BlockId, Locale};
    use liyasa_core::net::Url;

    use super::*;

    fn site() -> SiteMeta {
        SiteMeta {
            name: "Liyasa".to_owned(),
            canonical_origin: Url::parse("https://example.com").expect("a valid origin"),
            llms_txt: Url::parse("https://example.com/llms.txt").expect("a valid URL"),
            version: None,
            locale: Locale::new("en"),
        }
    }

    fn block(kind: BlockKind, children: Vec<Node>) -> Block {
        Block {
            id: BlockId::implicit("block", "", "", 0),
            explicit_id: None,
            kind,
            origin: Origin::default(),
            children,
        }
    }

    fn document(children: Vec<Node>) -> Block {
        block(BlockKind::Document, children)
    }

    /// One paragraph of `chars` characters, in 80-character lines so the
    /// result reads like prose rather than one enormous line.
    fn prose(chars: usize) -> Vec<Node> {
        let line = "Liyasa builds documentation that agents and people can both read well. ";
        let mut out = Vec::new();
        let mut written = 0;
        while written < chars {
            out.push(nodes::paragraph(line.trim_end()));
            written += line.len() + 1;
        }
        out
    }

    /// Long enough that 218 rows carry the page, as CM-145's fixture does.
    const GENERATED_NOTE: &str = "Generally available; provisioned capacity, \
        dual-stack networking, and same-region backups are enabled by default \
        for every account on this endpoint.";

    fn cell(text: &str) -> Node {
        Node::Block(block(
            BlockKind::TableCell,
            vec![Node::Inline(liyasa_core::document::Inline::Text(
                text.to_owned(),
            ))],
        ))
    }

    /// A generated endpoint table, four columns wide, the shape a reference
    /// page gets from an API description rather than from an author.
    fn table(rows: usize, note: &str) -> Node {
        let mut children = vec![Node::Block(block(
            BlockKind::TableRow { header: true },
            ["Region", "Endpoint", "Latency", "Notes"]
                .into_iter()
                .map(cell)
                .collect(),
        ))];
        for row in 0..rows {
            children.push(Node::Block(block(
                BlockKind::TableRow { header: false },
                vec![
                    cell(&format!("region-{row:04}")),
                    cell(&format!(
                        "https://api.example.com/v1/regions/region-{row:04}"
                    )),
                    cell(&format!("{}ms", 10 + row % 90)),
                    cell(note),
                ],
            )));
        }
        Node::Block(block(
            BlockKind::Table {
                align: vec![Align::None, Align::None, Align::None, Align::None],
            },
            children,
        ))
    }

    /// Renders a tree the way a page is rendered, so the measured size is the
    /// size an agent receives.
    fn render(root: &Block, registry: &Registry, site: &SiteMeta) -> String {
        let reference = liyasa_components::Reference::with(registry);
        // Measures the anonymous render, like the surface it measures.
        let shared = Shared::new(&reference).site(site).audience(Audience::Agent);
        let mut ctx = MarkdownCtx::with(shared);
        ctx.children(&root.children).expect("serializes");
        ctx.finish()
    }

    fn check_tree(root: &Block) -> (Report, Vec<String>) {
        let (registry, site) = (Registry::builtins(), site());
        let markdown = render(root, &registry, &site);
        let mut diagnostics = Diagnostics::new();
        let report = check(&markdown, root, &registry, &site, &mut diagnostics);
        let codes = diagnostics
            .iter()
            .map(|d| d.code.as_str().to_owned())
            .collect();
        (report, codes)
    }

    #[test]
    fn cm_144_a_page_under_the_warn_band_is_quiet() {
        let (report, codes) = check_tree(&document(prose(10_000)));
        assert!(report.chars < WARN_CHARS, "{}", report.chars);
        assert!(codes.is_empty(), "{codes:?}");
    }

    #[test]
    fn cm_144_sixty_thousand_characters_warn() {
        let mut children = vec![nodes::heading(2, "Overview")];
        children.extend(prose(30_000));
        children.push(nodes::heading(2, "Reference"));
        children.extend(prose(30_000));
        let (report, codes) = check_tree(&document(children));
        assert!(
            report.chars > WARN_CHARS && report.chars < ERROR_CHARS,
            "{}",
            report.chars
        );
        assert_eq!(codes, ["W0308"]);
    }

    #[test]
    fn cm_144_a_hundred_and_twenty_thousand_characters_fail_the_build() {
        let mut children = vec![nodes::heading(2, "Overview")];
        children.extend(prose(60_000));
        children.push(nodes::heading(2, "Reference"));
        children.extend(prose(60_000));
        let (report, codes) = check_tree(&document(children));
        assert!(report.chars > ERROR_CHARS, "{}", report.chars);
        assert_eq!(codes, ["E0307"]);
    }

    #[test]
    fn cm_144_the_diagnostic_suggests_h2_split_points() {
        let mut children = vec![nodes::heading(2, "Overview")];
        children.extend(prose(60_000));
        children.push(nodes::heading(2, "Reference"));
        children.extend(prose(60_000));
        let root = document(children);
        let (registry, site) = (Registry::builtins(), site());
        let markdown = render(&root, &registry, &site);
        let mut diagnostics = Diagnostics::new();
        check(&markdown, &root, &registry, &site, &mut diagnostics);
        let help = diagnostics
            .iter()
            .find_map(|d| d.help.as_deref())
            .expect("a suggestion");
        assert!(help.contains("Reference"), "{help}");
    }

    #[test]
    fn cm_144_a_fenced_heading_is_not_a_split_point() {
        let markdown = "start\n\n```md\n## Not a heading\n```\n\n## A heading\n";
        assert_eq!(suggest_splits(markdown, 1), ["A heading"]);
    }

    #[test]
    fn cm_145_a_generated_table_that_dominates_an_oversized_page_is_named() {
        let mut children = vec![nodes::heading(2, "Regions")];
        children.push(table(218, GENERATED_NOTE));
        children.extend(prose(17_000));
        let root = document(children);
        let (registry, site) = (Registry::builtins(), site());
        let markdown = render(&root, &registry, &site);
        let mut diagnostics = Diagnostics::new();
        let report = check(&markdown, &root, &registry, &site, &mut diagnostics);

        assert!(report.chars > WARN_CHARS, "{}", report.chars);
        let bulk = diagnostics
            .iter()
            .find(|d| d.code.as_str() == "W0321")
            .expect("W0321");
        assert!(bulk.message.contains("218-row table"), "{}", bulk.message);
        assert!(bulk.message.contains("## Regions"), "{}", bulk.message);
        let share = report.bulk[0].share(report.chars);
        assert!(share > 0.5, "{share}");
        assert!(
            bulk.message
                .contains(&format!("{}%", (share * 100.0).round() as u64)),
            "{}",
            bulk.message
        );
        let help = bulk.help.as_deref().expect("the fix options");
        for fix in BULK_FIXES {
            assert!(help.contains(fix), "{help}");
        }
    }

    #[test]
    fn cm_145_a_table_under_fifty_rows_is_not_bulk() {
        let mut children = vec![nodes::heading(2, "Regions")];
        children.push(table(40, GENERATED_NOTE));
        children.extend(prose(60_000));
        let (report, codes) = check_tree(&document(children));
        assert!(report.bulk.is_empty(), "{:?}", report.bulk);
        assert_eq!(codes, ["W0308"]);
    }

    #[test]
    fn cm_145_a_long_page_of_prose_is_not_blamed_on_a_small_table() {
        let mut children = vec![nodes::heading(2, "Regions")];
        children.push(table(60, "Generally available."));
        children.extend(prose(60_000));
        let (report, codes) = check_tree(&document(children));
        assert_eq!(report.bulk.len(), 1, "{:?}", report.bulk);
        assert!(
            report.bulk_share() < BULK_DOMINANCE,
            "{}",
            report.bulk_share()
        );
        assert_eq!(codes, ["W0308"]);
    }

    #[test]
    fn cm_145_a_base64_blob_is_bulk() {
        let blob = "QUJDREVGR0hJSktMTU5PUFFSU1RVVldYWVowMTIzNDU2Nzg5".repeat(200);
        let mut children = vec![nodes::heading(2, "Certificate")];
        children.push(nodes::code_block(Some("text"), &blob));
        children.extend(prose(10_000));
        let (report, _) = check_tree(&document(children));
        assert_eq!(report.bulk.len(), 1, "{:?}", report.bulk);
        assert_eq!(report.bulk[0].kind, BulkKind::Base64Run);
    }

    #[test]
    fn cm_145_bulk_on_a_page_under_the_warn_band_says_nothing() {
        let children = vec![nodes::heading(2, "Regions"), table(60, GENERATED_NOTE)];
        let (report, codes) = check_tree(&document(children));
        assert_eq!(report.bulk.len(), 1, "{:?}", report.bulk);
        assert!(report.chars < WARN_CHARS, "{}", report.chars);
        assert!(codes.is_empty(), "{codes:?}");
    }
}
