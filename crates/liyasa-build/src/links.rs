//! Link and image resolution (PRD §7.5, CM-35, CM-36).
//!
//! `liyasa-markdown` says which form an `href` is written in; only the build
//! knows the route table, the anchors each page has, and which files exist, so
//! resolution lands here. A reference that resolves is rewritten to the route
//! it points at and recorded on the node; one that does not is `E0401`,
//! `E0402`, or `E0403`.
//!
//! `page:id` reaches this pass on a Markdown link and nowhere else: the
//! sanitizer widens its allow list by `INTERNAL_SCHEMES` at that one site, so
//! an image `src` or a raw `<a href>` carrying `page:` is still `E0304`
//! (`plan/rfcs/0605-page-scheme-is-stripped.md`).

use std::collections::{BTreeMap, BTreeSet};

use liyasa_core::diagnostics::{Diagnostic, Diagnostics, code};
use liyasa_core::document::{Block, BlockKind, Inline, Node};
use liyasa_core::ids::{PageId, Route};
use liyasa_core::vfs::VfsPath;
use liyasa_markdown::ast::links::{self, Form};

/// Everything resolution compares a reference against.
#[derive(Debug, Default)]
pub struct Table {
    /// Every route the build serves.
    pub routes: BTreeSet<Route>,
    /// Heading anchors per route, for `#fragment` checks (`E0402`).
    pub anchors: BTreeMap<Route, BTreeSet<String>>,
    /// `page:<id>` targets (CM-36).
    pub by_id: BTreeMap<PageId, Route>,
    /// Files the asset pass copies, so an image reference can be checked.
    pub files: BTreeSet<VfsPath>,
    /// The route a page's own relative links are resolved against.
    pub base_path: String,
}

impl Table {
    pub fn has_route(&self, route: &Route) -> bool {
        self.routes.contains(route)
    }

    fn anchor(&self, route: &Route, anchor: &str) -> bool {
        self.anchors
            .get(route)
            .is_some_and(|anchors| anchors.contains(anchor))
    }
}

/// How strictly a missing target is reported. `build.strictLinks` decides.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Strictness {
    /// A missing target fails the build.
    Error,
    /// A missing target is a warning; the link is left as written.
    Warn,
}

/// Resolves every link and image on one page, in place.
pub fn resolve(
    root: &mut Block,
    page: &Route,
    page_source: &VfsPath,
    table: &Table,
    strictness: Strictness,
) -> Diagnostics {
    let mut diagnostics = Diagnostics::new();
    let anchors = collect_anchors(root);
    let mut pass = Pass {
        page,
        page_source,
        table,
        strictness,
        anchors,
        diagnostics: &mut diagnostics,
    };
    pass.block(root);
    diagnostics
}

/// The heading anchors of one document, which is what `E0402` checks a
/// same-page fragment against.
pub fn collect_anchors(root: &Block) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    walk_anchors(root, &mut out);
    out
}

fn walk_anchors(block: &Block, out: &mut BTreeSet<String>) {
    if let BlockKind::Heading { anchor, .. } = &block.kind {
        out.insert(anchor.clone());
    }
    if let BlockKind::Component { props, .. } = &block.kind
        && let Some(liyasa_core::document::PropValue::Str(id)) = props.get("id")
    {
        out.insert(id.clone());
    }
    for child in &block.children {
        if let Node::Block(child) = child {
            walk_anchors(child, out);
        }
    }
}

struct Pass<'a> {
    page: &'a Route,
    page_source: &'a VfsPath,
    table: &'a Table,
    strictness: Strictness,
    anchors: BTreeSet<String>,
    diagnostics: &'a mut Diagnostics,
}

impl Pass<'_> {
    fn block(&mut self, block: &mut Block) {
        for child in &mut block.children {
            match child {
                Node::Block(child) => self.block(child),
                Node::Inline(child) => self.inline(child),
            }
        }
    }

    fn inline(&mut self, inline: &mut Inline) {
        match inline {
            Inline::Link {
                href,
                children,
                resolved,
                ..
            } => {
                if let Some(route) = self.link(href) {
                    *resolved = Some(route);
                }
                for child in children {
                    self.inline(child);
                }
            }
            Inline::Image { src, dark, .. } => {
                self.image(src);
                if let Some(dark) = dark {
                    self.image(dark);
                }
            }
            Inline::Emph(children) | Inline::Strong(children) | Inline::Strike(children) => {
                for child in children {
                    self.inline(child);
                }
            }
            _ => {}
        }
    }

    /// Resolves one link, rewriting it to the route it means.
    fn link(&mut self, href: &mut String) -> Option<Route> {
        let anchor = links::anchor_of(href).map(str::to_owned);
        match links::form_of(href) {
            Form::External => None,
            Form::Anchor => {
                let anchor = anchor?;
                if !self.anchors.contains(&anchor) {
                    self.report(
                        code::E0402,
                        format!("`#{anchor}` is not a heading on `{}`", self.page),
                    );
                }
                None
            }
            // Only a Markdown link carries this form past the sanitizer, and
            // this is the pass that rewrites it (RFC 0605).
            Form::PageId => {
                let id = links::page_id_of(href)?;
                match PageId::parse(id).and_then(|id| self.table.by_id.get(&id).cloned()) {
                    Some(route) => {
                        *href = self.with_anchor(&route, anchor.as_deref());
                        Some(route)
                    }
                    None => {
                        self.report(code::E0401, format!("`page:{id}` is not a page"));
                        None
                    }
                }
            }
            Form::Route | Form::Relative => {
                let target = match links::form_of(href) {
                    Form::Relative => self.relative(href),
                    _ => Route::new(strip_anchor(href).trim_end_matches('/').to_owned()),
                };
                let target = match target.as_str().is_empty() {
                    true => Route::new("/"),
                    false => target,
                };
                if !self.table.has_route(&target) {
                    // A reference to a file the build copies is a download, not
                    // a broken page (CM-84).
                    if self.is_file(href) {
                        return None;
                    }
                    self.report(
                        code::E0401,
                        format!("`{href}` on `{}` is not a route", self.page),
                    );
                    return None;
                }
                if let Some(anchor) = &anchor
                    && !self.table.anchor(&target, anchor)
                    && !self.table.anchors.is_empty()
                {
                    self.report(
                        code::E0402,
                        format!("`#{anchor}` is not a heading on `{target}`"),
                    );
                }
                *href = self.with_anchor(&target, anchor.as_deref());
                Some(target)
            }
        }
    }

    fn image(&mut self, src: &mut String) {
        if links::form_of(src) == Form::External {
            return;
        }
        let path = self.file_path(src);
        if self.table.files.contains(&path) {
            *src = format!("{}/{}", self.table.base_path.trim_end_matches('/'), path);
            return;
        }
        self.diagnostics.push(
            Diagnostic::new(
                code::E0403,
                format!("`{src}` on `{}` is not a file this build has", self.page),
            )
            .help("check the path, or add the file under `assets/`"),
        );
    }

    /// A relative href against the page's own directory.
    fn relative(&self, href: &str) -> Route {
        let cleaned = strip_anchor(href);
        let directory = self
            .page
            .as_str()
            .trim_end_matches('/')
            .rsplit_once('/')
            .map(|(head, _)| head.to_owned())
            .unwrap_or_default();
        let joined = VfsPath::new(format!("{directory}/{cleaned}"));
        let text = joined
            .as_str()
            .trim_end_matches(".md")
            .trim_end_matches(".mdx")
            .trim_end_matches("/index")
            .to_owned();
        match text.is_empty() {
            true => Route::new("/"),
            false => Route::new(format!("/{text}")),
        }
    }

    /// Where a relative reference points in the project tree.
    fn file_path(&self, src: &str) -> VfsPath {
        let cleaned = strip_anchor(src);
        if cleaned.starts_with('/') {
            return VfsPath::new(cleaned);
        }
        let directory = self
            .page_source
            .parent()
            .map(|parent| parent.as_str().to_owned())
            .unwrap_or_default();
        VfsPath::new(format!("{directory}/{cleaned}"))
    }

    fn is_file(&self, href: &str) -> bool {
        self.table.files.contains(&self.file_path(href))
    }

    fn with_anchor(&self, route: &Route, anchor: Option<&str>) -> String {
        let base = self.table.base_path.trim_end_matches('/');
        match anchor {
            Some(anchor) => format!("{base}{route}#{anchor}"),
            None => format!("{base}{route}"),
        }
    }

    fn report(&mut self, code: liyasa_core::diagnostics::Code, message: String) {
        let diagnostic = Diagnostic::new(code, message);
        let diagnostic = match self.strictness {
            Strictness::Error => diagnostic,
            Strictness::Warn => {
                diagnostic.with_severity(liyasa_core::diagnostics::Severity::Warning)
            }
        };
        self.diagnostics.push(diagnostic);
    }
}

fn strip_anchor(href: &str) -> &str {
    href.split('#').next().unwrap_or(href)
}

#[cfg(test)]
mod tests {
    use liyasa_components::registry::Registry;
    use liyasa_core::markdown::ParseOptions;
    use liyasa_core::source_map::SourceMap;
    use std::sync::Arc;

    use super::*;

    fn document(text: &str) -> liyasa_core::document::Document {
        let mut map = SourceMap::new();
        let id = map.intern(VfsPath::new("guides/install.md"), Arc::from(text));
        let (source, _) = liyasa_markdown::scan(text, id);
        let context = liyasa_core::markdown::TemplateContext {
            values: minijinja::Value::UNDEFINED,
            tracking: false,
        };
        let environment = liyasa_markdown::source::expand::environment(&Default::default());
        let expanded =
            liyasa_markdown::source::expand::expand(&map, &source, &context, &environment)
                .expect("the fixture expands");
        liyasa_markdown::parse(&expanded, &Registry::builtins(), &ParseOptions::default())
    }

    fn table() -> Table {
        Table {
            routes: ["/", "/guides/install", "/guides/upgrade", "/reference/api"]
                .into_iter()
                .map(Route::new)
                .collect(),
            anchors: [(
                Route::new("/reference/api"),
                ["rate-limits".to_owned()].into_iter().collect(),
            )]
            .into_iter()
            .collect(),
            by_id: BTreeMap::new(),
            files: ["assets/hero.png", "guides/diagram.svg"]
                .into_iter()
                .map(VfsPath::new)
                .collect(),
            base_path: String::new(),
        }
    }

    fn resolve_fixture(text: &str) -> (liyasa_core::document::Document, Diagnostics) {
        let mut document = document(text);
        let diagnostics = resolve(
            &mut document.root,
            &Route::new("/guides/install"),
            &VfsPath::new("guides/install.md"),
            &table(),
            Strictness::Error,
        );
        (document, diagnostics)
    }

    fn hrefs(block: &Block, out: &mut Vec<String>) {
        for child in &block.children {
            match child {
                Node::Block(child) => hrefs(child, out),
                Node::Inline(Inline::Link { href, .. }) => out.push(href.clone()),
                Node::Inline(Inline::Image { src, .. }) => out.push(src.clone()),
                _ => {}
            }
        }
    }

    /// Every raw HTML block and inline of a document, concatenated.
    fn raw_html_of(document: &liyasa_core::document::Document) -> String {
        fn walk(block: &Block, out: &mut String) {
            if let BlockKind::HtmlBlock { html } = &block.kind {
                out.push_str(html);
            }
            for child in &block.children {
                match child {
                    Node::Block(child) => walk(child, out),
                    Node::Inline(Inline::HtmlInline(html)) => out.push_str(html),
                    _ => {}
                }
            }
        }
        let mut out = String::new();
        walk(&document.root, &mut out);
        out
    }

    fn links_of(document: &liyasa_core::document::Document) -> Vec<String> {
        let mut out = Vec::new();
        hrefs(&document.root, &mut out);
        out
    }

    #[test]
    fn a_relative_link_resolves_to_a_route() {
        let (document, diagnostics) = resolve_fixture("[Upgrade](./upgrade.md)\n");
        assert!(!diagnostics.has_errors(), "{diagnostics:?}");
        assert_eq!(links_of(&document), ["/guides/upgrade"]);
    }

    #[test]
    fn a_site_absolute_link_is_kept() {
        let (document, diagnostics) = resolve_fixture("[API](/reference/api)\n");
        assert!(!diagnostics.has_errors(), "{diagnostics:?}");
        assert_eq!(links_of(&document), ["/reference/api"]);
    }

    #[test]
    fn a_link_to_nothing_is_e0401() {
        let (_, diagnostics) = resolve_fixture("[Ghost](./ghost.md)\n");
        assert_eq!(
            diagnostics
                .iter()
                .map(|d| d.code.as_str())
                .collect::<Vec<_>>(),
            ["E0401"]
        );
    }

    #[test]
    fn a_fragment_on_another_page_is_checked() {
        let (_, ok) = resolve_fixture("[Limits](/reference/api#rate-limits)\n");
        assert!(!ok.has_errors(), "{ok:?}");

        let (_, bad) = resolve_fixture("[Limits](/reference/api#quotas)\n");
        assert_eq!(
            bad.iter().map(|d| d.code.as_str()).collect::<Vec<_>>(),
            ["E0402"]
        );
    }

    #[test]
    fn a_fragment_on_this_page_is_checked_against_its_own_headings() {
        let (_, ok) = resolve_fixture("# Install\n\n## Steps\n\n[Steps](#steps)\n");
        assert!(!ok.has_errors(), "{ok:?}");

        let (_, bad) = resolve_fixture("# Install\n\n[Nowhere](#nowhere)\n");
        assert_eq!(
            bad.iter().map(|d| d.code.as_str()).collect::<Vec<_>>(),
            ["E0402"]
        );
    }

    #[test]
    fn an_external_link_is_left_alone() {
        let (document, diagnostics) =
            resolve_fixture("[Status](https://status.acme.com) and [Mail](mailto:a@b.c)\n");
        assert!(!diagnostics.has_errors(), "{diagnostics:?}");
        assert_eq!(
            links_of(&document),
            ["https://status.acme.com", "mailto:a@b.c"]
        );
    }

    #[test]
    fn an_image_resolves_to_the_file_it_names() {
        let (document, diagnostics) = resolve_fixture("![A diagram](./diagram.svg)\n");
        assert!(!diagnostics.has_errors(), "{diagnostics:?}");
        assert_eq!(links_of(&document), ["/guides/diagram.svg"]);
    }

    #[test]
    fn a_missing_image_is_e0403() {
        let (_, diagnostics) = resolve_fixture("![Gone](./missing.png)\n");
        assert_eq!(
            diagnostics
                .iter()
                .map(|d| d.code.as_str())
                .collect::<Vec<_>>(),
            ["E0403"]
        );
    }

    #[test]
    fn a_link_to_a_download_is_not_a_broken_page() {
        let (_, diagnostics) = resolve_fixture("[Diagram](./diagram.svg)\n");
        assert!(!diagnostics.has_errors(), "{diagnostics:?}");
    }

    /// A link node as the parser would have produced it if the sanitizer let
    /// the scheme through (`plan/rfcs/0605-page-scheme-is-stripped.md`).
    fn link_node(href: &str) -> Block {
        Block {
            id: liyasa_core::ids::BlockId([0; 12]),
            explicit_id: None,
            kind: BlockKind::Paragraph,
            origin: liyasa_core::document::Origin::default(),
            children: vec![Node::Inline(Inline::Link {
                href: href.to_owned(),
                title: None,
                children: Vec::new(),
                resolved: None,
            })],
        }
    }

    #[test]
    fn a_page_id_link_survives_a_rename() {
        let id = PageId(ulid::Ulid::from_parts(1, 2));
        let mut table = table();
        table.by_id.insert(id, Route::new("/guides/upgrade"));
        let mut root = link_node(&format!("page:{id}"));
        let diagnostics = resolve(
            &mut root,
            &Route::new("/guides/install"),
            &VfsPath::new("guides/install.md"),
            &table,
            Strictness::Error,
        );
        assert!(!diagnostics.has_errors(), "{diagnostics:?}");
        let mut hrefs = Vec::new();
        super::tests::hrefs(&root, &mut hrefs);
        assert_eq!(hrefs, ["/guides/upgrade"]);
    }

    #[test]
    fn an_unknown_page_id_is_e0401() {
        let id = PageId(ulid::Ulid::from_parts(3, 4));
        let mut root = link_node(&format!("page:{id}"));
        let diagnostics = resolve(
            &mut root,
            &Route::new("/guides/install"),
            &VfsPath::new("guides/install.md"),
            &table(),
            Strictness::Error,
        );
        assert_eq!(
            diagnostics
                .iter()
                .map(|d| d.code.as_str())
                .collect::<Vec<_>>(),
            ["E0401"]
        );
    }

    #[test]
    fn a_markdown_page_link_reaches_resolution_and_becomes_a_route() {
        let id = PageId(ulid::Ulid::from_parts(1, 2));
        let mut table = table();
        table.by_id.insert(id, Route::new("/guides/upgrade"));
        let mut parsed = document(&format!("[Upgrade](page:{id})\n"));
        // The sanitizer lets `page:` through on a Markdown link, and only
        // there (`plan/rfcs/0605-page-scheme-is-stripped.md`).
        assert_eq!(links_of(&parsed), [format!("page:{id}")]);

        let diagnostics = resolve(
            &mut parsed.root,
            &Route::new("/guides/install"),
            &VfsPath::new("guides/install.md"),
            &table,
            Strictness::Error,
        );
        assert!(!diagnostics.has_errors(), "{diagnostics:?}");
        assert_eq!(links_of(&parsed), ["/guides/upgrade"]);
    }

    /// The asymmetry is deliberate: `liyasa_build::links` rewrites a Markdown
    /// link and nothing else, so an image `src` or a raw `<a href>` carrying
    /// `page:` would reach a reader as a dead URL. Both are rejected during
    /// parse, and this test exists so that nobody "fixes" the inconsistency by
    /// putting `page` in `SCHEMES`.
    #[test]
    fn an_image_src_and_raw_html_still_refuse_the_page_scheme() {
        let id = PageId(ulid::Ulid::from_parts(1, 2));

        let image = document(&format!("![Diagram](page:{id})\n"));
        assert_eq!(links_of(&image), [""], "an image src is cleared");
        assert!(
            image
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code.as_str() == "E0304"),
            "{:?}",
            image.diagnostics
        );

        // The control: raw HTML does reach the build, and an ordinary scheme
        // survives it, so the assertion below is about `page:` and not about
        // an empty haystack.
        let kept = document("<a href=\"https://status.acme.com\">Status</a>\n");
        assert!(
            raw_html_of(&kept).contains("https://status.acme.com"),
            "{}",
            raw_html_of(&kept)
        );

        let raw = document(&format!("<a href=\"page:{id}\">Upgrade</a>\n"));
        let html = raw_html_of(&raw);
        assert!(
            !html.contains("page:"),
            "raw html keeps no `page:` href: {html}"
        );
    }

    #[test]
    fn lenient_link_checking_warns_instead_of_failing() {
        let mut document = document("[Ghost](./ghost.md)\n");
        let diagnostics = resolve(
            &mut document.root,
            &Route::new("/guides/install"),
            &VfsPath::new("guides/install.md"),
            &table(),
            Strictness::Warn,
        );
        assert!(!diagnostics.has_errors(), "{diagnostics:?}");
        assert_eq!(diagnostics.len(), 1);
    }

    #[test]
    fn a_base_path_prefixes_a_resolved_link() {
        let mut table = table();
        table.base_path = "/docs".to_owned();
        let mut document = document("[API](/reference/api)\n");
        let diagnostics = resolve(
            &mut document.root,
            &Route::new("/guides/install"),
            &VfsPath::new("guides/install.md"),
            &table,
            Strictness::Error,
        );
        assert!(!diagnostics.has_errors(), "{diagnostics:?}");
        assert_eq!(links_of(&document), ["/docs/reference/api"]);
    }
}
