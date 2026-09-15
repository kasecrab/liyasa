//! The page pipeline (PRD §6.6 queries 4, 5, 8, and 9).
//!
//! Expansion, parse, and serialization in one place, because they are one
//! query chain: `page_expanded` feeds `page_ast`, which feeds `page_html` and
//! `page_markdown`. This is the first caller in the workspace to run all four,
//! so the seams it uses are the ones the other crates left open: the AST walk
//! is `liyasa_markdown::render::html`, and components render through
//! `liyasa_components`' sinks rather than through `Renderer`, which needs a
//! `RenderCtx` no crate can build (`plan/rfcs/0302-render-ctx-is-unconstructible.md`).

pub mod blocks;

use liyasa_components::registry::Registry;
use liyasa_core::diagnostics::{Diagnostic, Diagnostics, code};
use liyasa_core::document::{Deps, Document, SourceDocument};
use liyasa_core::markdown::{
    Audience, Expanded, ExpansionRecord, HtmlMode, ParseOptions, SiteMeta, TemplateContext,
};
use liyasa_core::source_map::SourceMap;
use liyasa_markdown::source::expand::{Budget, ExpandOptions, Undefined};

pub use blocks::Blocks;

/// Everything the pipeline needs that is not the page itself.
pub struct Options<'a> {
    pub registry: &'a Registry,
    pub site: &'a SiteMeta,
    pub parse: ParseOptions,
    pub expand: ExpandOptions,
    /// The CSP nonce of this response (RX-110); empty for a static build.
    pub nonce: &'a str,
    /// Link and image resolution (CM-35, CM-36). `None` renders the AST as
    /// written, which is what a preview of a single page wants.
    pub resolve: Option<Resolve<'a>>,
}

/// What link resolution needs beyond the page itself.
#[derive(Clone, Copy)]
pub struct Resolve<'a> {
    pub table: &'a crate::links::Table,
    pub route: &'a liyasa_core::ids::Route,
    pub source_path: &'a liyasa_core::vfs::VfsPath,
    pub strictness: crate::links::Strictness,
}

impl<'a> Options<'a> {
    /// Build-time defaults: strict undefined, the per-page template budget.
    pub fn new(registry: &'a Registry, site: &'a SiteMeta) -> Self {
        Self {
            registry,
            site,
            parse: ParseOptions::default(),
            expand: ExpandOptions {
                budget: Budget::BUILD,
                undefined: Undefined::Strict,
            },
            nonce: "",
            resolve: None,
        }
    }

    /// §6.6.4: the anonymous render every shared index and agent surface reads.
    /// Undefined `reader.*` fields render as nothing whatever the config says.
    pub fn anonymous(mut self) -> Self {
        self.expand.undefined = Undefined::Lenient;
        self
    }

    /// §6.6.3 item 5: the request-path budget for an on-demand render.
    pub fn on_demand(mut self) -> Self {
        self.expand.budget = Budget::REQUEST;
        self
    }

    pub fn html_mode(mut self, mode: HtmlMode) -> Self {
        self.parse.html = mode;
        self
    }

    pub fn nonce(mut self, nonce: &'a str) -> Self {
        self.nonce = nonce;
        self
    }

    pub fn resolving(mut self, resolve: Resolve<'a>) -> Self {
        self.resolve = Some(resolve);
        self
    }
}

/// One page in every serialization the build needs.
#[derive(Debug, Clone, Default)]
pub struct Page {
    pub html: String,
    pub markdown: String,
    /// What the search index stores.
    pub text: String,
    /// The Rendered AST this page was serialized from, kept so the agent
    /// surfaces (§11.7) serialize the same render the HTML came from rather
    /// than a second one.
    pub document: Option<Document>,
    pub deps: Deps,
    /// What the page read while it expanded, which decides its variants
    /// (§6.6.3 item 1).
    pub record: ExpansionRecord,
    pub diagnostics: Diagnostics,
}

impl Page {
    pub fn has_errors(&self) -> bool {
        self.diagnostics.has_errors()
    }
}

/// Expands, parses, and serializes one page.
///
/// An expansion failure is not a panic and not an empty page: the diagnostics
/// come back and the caller decides whether the build fails, which is what
/// `--strict` and the dev server's error overlay each need to do differently.
pub fn page(
    map: &SourceMap,
    source: &SourceDocument,
    context: &TemplateContext,
    options: &Options<'_>,
) -> Page {
    let environment = liyasa_markdown::source::expand::environment(&options.expand);
    let expanded = match liyasa_markdown::source::expand::expand_with(
        map,
        source,
        context,
        &environment,
        &options.expand,
    ) {
        Ok(expanded) => expanded,
        Err(diagnostics) => {
            return Page {
                diagnostics,
                ..Page::default()
            };
        }
    };
    from_expanded(&expanded, options)
}

/// The half of [`page`] after expansion, exposed because a dynamic page
/// re-expands from a cached Source Document and reuses everything after it
/// (§6.6.4).
pub fn from_expanded(expanded: &Expanded, options: &Options<'_>) -> Page {
    let record = expanded.record.clone();
    let mut document = liyasa_markdown::parse(expanded, options.registry, &options.parse);
    let mut resolution = Diagnostics::new();
    if let Some(resolve) = &options.resolve {
        resolution = crate::links::resolve(
            &mut document.root,
            resolve.route,
            resolve.source_path,
            resolve.table,
            resolve.strictness,
        );
    }
    let mut page = serialize(&document, options);
    page.diagnostics.extend(resolution.into_vec());
    page.record = record;
    page
}

fn serialize(document: &Document, options: &Options<'_>) -> Page {
    let mut blocks = Blocks::new(options.registry, options.site).nonce(options.nonce);
    let html = liyasa_markdown::render::html::render(&document.root, &mut blocks);
    let mut diagnostics = document.diagnostics.clone();
    diagnostics.extend(blocks.take_diagnostics().into_vec());

    Page {
        html,
        markdown: liyasa_markdown::render_markdown(document, Audience::Human, options.site),
        text: liyasa_markdown::render::render_text(document),
        deps: document.deps.clone(),
        document: Some(document.clone()),
        record: ExpansionRecord::default(),
        diagnostics,
    }
}

/// The Markdown an agent fetches (§11.7). Kept beside the HTML so a caller
/// cannot serialize one from a different render than the other.
pub fn agent_markdown(document: &Document, site: &SiteMeta) -> String {
    liyasa_markdown::render_markdown(document, Audience::Agent, site)
}

/// Reports a render that produced nothing a reader could use.
pub fn empty_page(route: &str) -> Diagnostic {
    Diagnostic::new(code::E0701, format!("`{route}` rendered to an empty page"))
        .help("the page expanded to nothing; check its template statements")
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use liyasa_core::ids::Locale;
    use liyasa_core::net::Url;
    use liyasa_core::vfs::VfsPath;

    use super::*;

    fn site() -> SiteMeta {
        SiteMeta {
            name: "Acme docs".to_owned(),
            canonical_origin: Url::parse("https://docs.acme.com").expect("an origin"),
            llms_txt: Url::parse("https://docs.acme.com/llms.txt").expect("a url"),
            version: None,
            locale: Locale::new("en"),
        }
    }

    fn context() -> TemplateContext {
        TemplateContext {
            values: minijinja::context! {
                site => minijinja::context! { name => "Acme" },
                page => minijinja::context! { title => "Install" },
            },
            tracking: false,
        }
    }

    fn rendered(text: &str) -> Page {
        let mut map = SourceMap::new();
        let id = map.intern(VfsPath::new("install.md"), Arc::from(text));
        let (source, scan) = liyasa_markdown::scan(text, id);
        assert!(!scan.has_errors(), "the fixture scans: {scan:?}");
        let registry = Registry::builtins();
        let site = site();
        page(&map, &source, &context(), &Options::new(&registry, &site))
    }

    #[test]
    fn a_page_becomes_html_markdown_and_text() {
        let page = rendered("---\ntitle: Install\n---\n# Install\n\nRun the installer.\n");
        assert!(page.html.contains("<h1"), "{}", page.html);
        assert!(page.html.contains("Run the installer."), "{}", page.html);
        assert!(page.markdown.contains("Run the installer."));
        assert!(page.text.contains("Run the installer."));
        assert!(!page.has_errors(), "{:?}", page.diagnostics);
    }

    #[test]
    fn a_template_expression_is_expanded_before_the_markdown_is_parsed() {
        let page = rendered("Welcome to {{ site.name }}.\n");
        assert!(page.html.contains("Welcome to Acme."), "{}", page.html);
        assert!(!page.html.contains("{{"), "{}", page.html);
    }

    #[test]
    fn a_directive_renders_through_the_component_registry() {
        let page = rendered(":::note\nMind the gap.\n:::\n");
        assert!(page.html.contains("Mind the gap."), "{}", page.html);
        assert!(page.html.contains("ly-callout"), "{}", page.html);
    }

    #[test]
    fn a_code_fence_keeps_its_body_and_its_language() {
        let page = rendered("```rust title=\"main.rs\"\nfn main() {}\n```\n");
        assert!(page.html.contains("ly-code"), "{}", page.html);
        assert!(page.html.contains("data-lang=\"rust\""), "{}", page.html);
        assert!(page.html.contains("fn main()"), "{}", page.html);
        assert!(page.html.contains("main.rs"), "{}", page.html);
    }

    #[test]
    fn an_undefined_name_is_a_diagnostic_at_build_strictness() {
        let page = rendered("Hello {{ nobody.at.all }}.\n");
        assert!(page.has_errors(), "{:?}", page.diagnostics);
        assert!(page.html.is_empty());
    }

    #[test]
    fn the_anonymous_render_leaves_an_undefined_reader_field_empty() {
        let text = "---\npersonalized: true\n---\nHello {{ reader.name }}.\n";
        let mut map = SourceMap::new();
        let id = map.intern(VfsPath::new("install.md"), Arc::from(text));
        let (source, _) = liyasa_markdown::scan(text, id);
        let registry = Registry::builtins();
        let site = site();
        let options = Options::new(&registry, &site).anonymous();
        let page = page(&map, &source, &context(), &options);
        assert!(!page.has_errors(), "{:?}", page.diagnostics);
        assert!(page.html.contains("Hello"), "{}", page.html);
        assert!(!page.html.contains("name"), "{}", page.html);
    }

    #[test]
    fn expansion_records_what_the_page_read() {
        let page = rendered("Welcome to {{ site.name }}.\n");
        assert!(!page.has_errors(), "{:?}", page.diagnostics);
        assert!(page.record.reader_fields.is_empty());
        assert!(page.record.env.is_empty());
    }
}
