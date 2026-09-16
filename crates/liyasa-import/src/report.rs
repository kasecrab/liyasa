//! The migration report (MIG-06).
//!
//! An import is judged by what it could not do. Every construct the importer
//! left for a human is one [`Attention`] item, a page with none of them scores
//! 100, and MIG-01's quality bar is the share of pages that reach 100.

use liyasa_core::diagnostics::{Code, Diagnostics, code};
use liyasa_core::span::Span;
use liyasa_core::vfs::VfsPath;
use serde::Serialize;

/// Which importer produced a report.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Source {
    Mintlify,
    Docusaurus,
    Mdx,
}

impl Source {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Mintlify => "mintlify",
            Self::Docusaurus => "docusaurus",
            Self::Mdx => "mdx",
        }
    }
}

impl std::fmt::Display for Source {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// What kind of construct was left for a human.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Kind {
    /// A component with no Liyasa equivalent, custom or from another product.
    CustomComponent,
    /// A JavaScript expression container: `{items.map(…)}`.
    Expression,
    /// An `import` or `export` statement.
    Module,
    /// A source config key with no Liyasa equivalent.
    ConfigKey,
    /// A navigation entry naming a page that is not in the source project.
    DanglingPage,
    /// The converted page does not scan as Liyasa Markdown. Almost always a
    /// page that was already malformed: an unclosed fence renders as one thing
    /// in a lenient MDX pipeline and as another here, and guessing which the
    /// author meant is not the importer's call.
    Malformed,
}

impl Kind {
    /// The registered code this kind is reported under.
    ///
    /// An attention item is user-facing output, so it carries a code an
    /// operator can look up, the same as any other diagnostic.
    pub const fn code(self) -> Code {
        match self {
            Self::CustomComponent => code::W1110,
            Self::Expression => code::W1111,
            Self::Module => code::W1112,
            Self::ConfigKey => code::W1113,
            Self::DanglingPage => code::W1114,
            Self::Malformed => code::W1116,
        }
    }

    /// What one item of this kind costs a page's confidence.
    ///
    /// A page carrying a custom component is wrong until someone writes that
    /// component, so the cost is most of the score; a config key the importer
    /// skipped leaves the prose intact, so it costs little.
    pub const fn cost(self) -> u8 {
        match self {
            Self::CustomComponent => 40,
            Self::Expression => 30,
            Self::Module => 20,
            Self::Malformed => 30,
            Self::DanglingPage => 10,
            Self::ConfigKey => 5,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CustomComponent => "custom component",
            Self::Expression => "JavaScript expression",
            Self::Module => "import or export",
            Self::ConfigKey => "unmapped config key",
            Self::DanglingPage => "missing page",
            Self::Malformed => "malformed Markdown",
        }
    }
}

/// One construct that needs manual attention.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Attention {
    pub kind: Kind,
    /// What was found, in the author's own spelling.
    pub what: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub span: Option<Span>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub help: Option<String>,
}

impl Attention {
    pub fn new(kind: Kind, what: impl Into<String>) -> Self {
        Self {
            kind,
            what: what.into(),
            span: None,
            help: None,
        }
    }

    #[must_use]
    pub fn at(mut self, span: Span) -> Self {
        self.span = Some(span);
        self
    }

    #[must_use]
    pub fn help(mut self, text: impl Into<String>) -> Self {
        self.help = Some(text.into());
        self
    }
}

/// What became of one page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PageReport {
    pub from: VfsPath,
    pub to: VfsPath,
    /// The route the page served in the source site.
    pub old_route: String,
    /// The route it serves in the imported site.
    pub route: String,
    pub attention: Vec<Attention>,
}

impl PageReport {
    pub fn new(
        from: VfsPath,
        to: VfsPath,
        old_route: impl Into<String>,
        route: impl Into<String>,
    ) -> Self {
        Self {
            from,
            to,
            old_route: old_route.into(),
            route: route.into(),
            attention: Vec::new(),
        }
    }

    /// 100 when nothing needs manual attention, down to 0.
    pub fn confidence(&self) -> u8 {
        self.attention
            .iter()
            .fold(100u8, |left, item| left.saturating_sub(item.kind.cost()))
    }

    /// Whether the page converted with no manual-attention item, which is what
    /// MIG-01's 99% is measured against.
    pub fn is_clean(&self) -> bool {
        self.attention.is_empty()
    }

    /// Whether the route changed, which is what makes a redirect necessary.
    pub fn moved(&self) -> bool {
        self.route != self.old_route
    }
}

/// A redirect from a source-site URL to the imported one (MIG-05).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Redirect {
    pub source: String,
    pub destination: String,
}

/// What one import produced.
#[derive(Debug, Clone, Serialize)]
pub struct Report {
    pub source: Source,
    pub pages: Vec<PageReport>,
    /// Files carried over unchanged: assets, specs, snippets.
    pub carried: Vec<VfsPath>,
    pub redirects: Vec<Redirect>,
    /// Problems that belong to the project rather than to one page.
    pub attention: Vec<Attention>,
    #[serde(skip)]
    pub diagnostics: Diagnostics,
}

impl Report {
    pub fn new(source: Source) -> Self {
        Self {
            source,
            pages: Vec::new(),
            carried: Vec::new(),
            redirects: Vec::new(),
            attention: Vec::new(),
            diagnostics: Diagnostics::new(),
        }
    }

    pub fn clean_pages(&self) -> usize {
        self.pages.iter().filter(|page| page.is_clean()).count()
    }

    /// The share of pages that converted with nothing left for a human, in
    /// percent. An import with no pages is not a passing import, so it is 0.
    pub fn clean_percent(&self) -> f64 {
        if self.pages.is_empty() {
            return 0.0;
        }
        self.clean_pages() as f64 * 100.0 / self.pages.len() as f64
    }

    pub fn mean_confidence(&self) -> u8 {
        if self.pages.is_empty() {
            return 0;
        }
        let total: u32 = self
            .pages
            .iter()
            .map(|page| u32::from(page.confidence()))
            .sum();
        (total / self.pages.len() as u32) as u8
    }

    /// Every page that needs a human, worst first.
    pub fn needs_attention(&self) -> Vec<&PageReport> {
        let mut pages: Vec<&PageReport> =
            self.pages.iter().filter(|page| !page.is_clean()).collect();
        pages.sort_by_key(|page| (page.confidence(), page.from.as_str().to_owned()));
        pages
    }

    /// The report a human reads.
    pub fn to_markdown(&self) -> String {
        let mut out = format!("# {} import\n\n", self.source);
        out.push_str(&format!(
            "{} pages, {} clean ({:.1}%), mean confidence {}.\n",
            self.pages.len(),
            self.clean_pages(),
            self.clean_percent(),
            self.mean_confidence(),
        ));
        if !self.carried.is_empty() {
            out.push_str(&format!(
                "{} files carried unchanged.\n",
                self.carried.len()
            ));
        }
        if !self.redirects.is_empty() {
            out.push_str(&format!(
                "{} redirects generated from moved pages ({}).\n",
                self.redirects.len(),
                code::W1115,
            ));
        }

        if !self.attention.is_empty() {
            out.push_str("\n## The project\n\n");
            for item in &self.attention {
                out.push_str(&line(item));
            }
        }

        let pages = self.needs_attention();
        if pages.is_empty() {
            out.push_str("\nEvery page converted with nothing left to do.\n");
            return out;
        }

        out.push_str("\n## Pages that need attention\n\n");
        for page in pages {
            out.push_str(&format!("### {} ({})\n\n", page.from, page.confidence()));
            for item in &page.attention {
                out.push_str(&line(item));
            }
            out.push('\n');
        }
        out
    }
}

fn line(item: &Attention) -> String {
    let mut out = format!(
        "- {} {}: `{}`",
        item.kind.code(),
        item.kind.as_str(),
        item.what
    );
    if let Some(help) = &item.help {
        out.push_str(&format!(" — {help}"));
    }
    out.push('\n');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(name: &str) -> PageReport {
        PageReport::new(
            VfsPath::new(format!("{name}.mdx")),
            VfsPath::new(format!("{name}.md")),
            format!("/{name}"),
            format!("/{name}"),
        )
    }

    #[test]
    fn a_page_with_nothing_left_scores_full_confidence() {
        let page = page("install");
        assert_eq!(page.confidence(), 100);
        assert!(page.is_clean());
    }

    #[test]
    fn each_item_costs_its_kind() {
        let mut page = page("install");
        page.attention
            .push(Attention::new(Kind::CustomComponent, "PricingTable"));
        assert_eq!(page.confidence(), 60);
        page.attention
            .push(Attention::new(Kind::Expression, "{items.map(…)}"));
        assert_eq!(page.confidence(), 30);
        assert!(!page.is_clean());
    }

    #[test]
    fn confidence_floors_at_zero_rather_than_wrapping() {
        let mut page = page("kitchen-sink");
        for _ in 0..10 {
            page.attention
                .push(Attention::new(Kind::CustomComponent, "Widget"));
        }
        assert_eq!(page.confidence(), 0);
    }

    #[test]
    fn a_page_whose_route_changed_is_moved() {
        let mut page = page("install");
        assert!(!page.moved());
        page.route = "/guides/install".to_owned();
        assert!(page.moved());
    }

    #[test]
    fn the_clean_share_is_what_the_quality_bar_measures() {
        let mut report = Report::new(Source::Mintlify);
        for at in 0..100 {
            report.pages.push(page(&format!("page-{at}")));
        }
        assert_eq!(report.clean_percent(), 100.0);
        report.pages[0]
            .attention
            .push(Attention::new(Kind::CustomComponent, "Widget"));
        assert_eq!(report.clean_percent(), 99.0);
        assert_eq!(report.clean_pages(), 99);
    }

    #[test]
    fn an_empty_import_does_not_score_a_perfect_run() {
        let report = Report::new(Source::Mintlify);
        assert_eq!(report.clean_percent(), 0.0);
        assert_eq!(report.mean_confidence(), 0);
    }

    #[test]
    fn the_worst_page_is_listed_first() {
        let mut report = Report::new(Source::Mdx);
        let mut bad = page("bad");
        bad.attention
            .push(Attention::new(Kind::CustomComponent, "Widget"));
        let mut worse = page("worse");
        worse
            .attention
            .push(Attention::new(Kind::CustomComponent, "Widget"));
        worse
            .attention
            .push(Attention::new(Kind::Expression, "{x}"));
        report.pages.push(bad);
        report.pages.push(worse);
        report.pages.push(page("fine"));

        let listed = report.needs_attention();
        assert_eq!(listed.len(), 2);
        assert_eq!(listed[0].from.as_str(), "worse.mdx");
    }

    #[test]
    fn every_kind_reports_under_its_registered_code() {
        // A code claimed in codes.toml and never emitted is a registry entry
        // with no behaviour behind it, and a documentation page nobody can
        // reach. Every kind an operator can see carries one.
        for (kind, expected) in [
            (Kind::CustomComponent, "W1110"),
            (Kind::Expression, "W1111"),
            (Kind::Module, "W1112"),
            (Kind::ConfigKey, "W1113"),
            (Kind::DanglingPage, "W1114"),
            (Kind::Malformed, "W1116"),
        ] {
            assert_eq!(kind.code().as_str(), expected);
        }
    }

    #[test]
    fn the_report_prints_the_code_beside_each_item() {
        let mut report = Report::new(Source::Mintlify);
        let mut bad = page("pricing");
        bad.attention
            .push(Attention::new(Kind::CustomComponent, "PricingTable"));
        report.pages.push(bad);
        report.redirects.push(Redirect {
            source: "/old".to_owned(),
            destination: "/new".to_owned(),
        });

        let text = report.to_markdown();
        assert!(
            text.contains("W1110 custom component: `PricingTable`"),
            "{text}"
        );
        assert!(
            text.contains("1 redirects generated from moved pages (W1115)."),
            "{text}"
        );
    }

    #[test]
    fn the_markdown_report_names_every_construct() {
        let mut report = Report::new(Source::Mintlify);
        let mut bad = page("pricing");
        bad.attention.push(
            Attention::new(Kind::CustomComponent, "PricingTable")
                .help("write it as a user-defined component"),
        );
        report.pages.push(bad);
        report.pages.push(page("install"));

        let text = report.to_markdown();
        assert!(text.contains("# mintlify import"));
        assert!(text.contains("2 pages, 1 clean (50.0%)"));
        assert!(text.contains("PricingTable"));
        assert!(text.contains("write it as a user-defined component"));
    }

    #[test]
    fn a_clean_report_says_so_rather_than_printing_an_empty_list() {
        let mut report = Report::new(Source::Docusaurus);
        report.pages.push(page("install"));
        assert!(
            report
                .to_markdown()
                .contains("Every page converted with nothing left to do.")
        );
    }
}
