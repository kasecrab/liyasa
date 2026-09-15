//! The Markdown an agent fetches from `<route>.md` (PRD §11.7, CM-140..143,
//! RX-63, RX-65).
//!
//! The AST walk itself belongs to `liyasa-markdown` and is borrowed from
//! `liyasa-components` until it lands; this module is the layer around it — the
//! discovery directive, the front-matter reduction, the agent notes, and the
//! absolute links (`plan/rfcs/1001-agent-markdown-serializer.md`).

pub mod links;

use std::collections::BTreeSet;

use liyasa_components::registry::Registry;
use liyasa_components::render::{MarkdownCtx, Shared};
use liyasa_core::diagnostics::{Diagnostic, Diagnostics, code};
use liyasa_core::document::{Block, BlockKind, Document, Node};
use liyasa_core::frontmatter::{AiSetting, FrontmatterFields};
use liyasa_core::ids::Route;
use liyasa_core::markdown::{Audience, SiteMeta};

use crate::agents::site::CanonicalOrigin;

pub use links::Links;

/// The section CM-142 appends, and the heading the spec's readers look for.
pub const AGENT_NOTES_HEADING: &str = "Notes for agents";

/// The line CM-141 puts before everything else. The spec check
/// `llms-txt-directive-md` matches on the `llms.txt` URL in it.
pub fn discovery_directive(llms_txt: &str) -> String {
    format!("> For AI agents: a documentation index is available at {llms_txt}")
}

/// What one page's Markdown is rendered from.
pub struct Options<'a> {
    pub site: &'a SiteMeta,
    pub registry: &'a Registry,
    pub route: &'a Route,
    pub frontmatter: Option<&'a FrontmatterFields>,
    /// Routes the site publishes, so an internal link takes its `.md` form.
    pub routes: &'a BTreeSet<Route>,
    /// `agents.markdown.instructions` (CM-142).
    pub site_instructions: Option<&'a str>,
    /// The operation schema `agents.markdown.includeOpenApiSchema` asks for,
    /// already serialized by `liyasa-openapi` (RX-63). `None` when the key is
    /// off or the page documents no operation.
    pub openapi_schema: Option<&'a str>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Page {
    pub markdown: String,
    pub diagnostics: Diagnostics,
}

/// Renders one page for `Audience::Agent`.
///
/// The document is the anonymous render (§6.6.4): a personalized page never
/// reaches this function, because no reader value may enter a shared agent
/// surface (SRC-12).
pub fn render_page(doc: &Document, options: &Options<'_>) -> Page {
    let mut diagnostics = Diagnostics::new();
    let mut root = doc.root.clone();

    match CanonicalOrigin::parse(options.site.canonical_origin.as_str()) {
        Some(origin) => {
            let links = Links {
                origin: &origin,
                route: options.route,
                routes: options.routes,
            };
            links::absolutize(&mut root, &links, &mut diagnostics);
        }
        None => diagnostics.push(Diagnostic::new(
            code::W0131,
            format!(
                "`{}` is not an origin absolute URLs can be built from",
                options.site.canonical_origin
            ),
        )),
    }

    let title = title_of(options.frontmatter, &root);
    let body = body_of(&root, title.as_deref());

    let reference = liyasa_components::Reference::with(options.registry);
    let shared = Shared::new(&reference)
        .site(options.site)
        .audience(Audience::Agent);
    let mut ctx = MarkdownCtx::with(shared);

    // CM-141: the directive is the first line, before the H1, so a pipeline
    // that truncates a page still carries the way back to the index.
    ctx.out
        .paragraph(&discovery_directive(options.site.llms_txt.as_str()));
    if let Some(title) = &title {
        ctx.out.heading(1, title);
    }
    if let Some(description) = options
        .frontmatter
        .and_then(|f| f.description.as_deref())
        .map(str::trim)
        .filter(|d| !d.is_empty())
    {
        ctx.out.paragraph(description);
    }

    if let Err(error) = ctx.children(&body) {
        diagnostics.push(Diagnostic::new(
            code::E0701,
            format!(
                "`{}` could not be serialized as Markdown: {error}",
                options.route
            ),
        ));
    }

    if let Some(schema) = options.openapi_schema {
        ctx.out.heading(2, "OpenAPI schema");
        ctx.out.fence("json", schema);
    }

    let notes = agent_notes(options.site_instructions, options.frontmatter);
    if !notes.is_empty() {
        ctx.out.heading(2, AGENT_NOTES_HEADING);
        for note in notes {
            ctx.out.paragraph(&note);
        }
    }

    diagnostics.extend(std::mem::take(&mut ctx.shared.diagnostics));
    Page {
        markdown: ctx.finish(),
        diagnostics,
    }
}

/// The page title: front matter first, then the document's own leading H1.
fn title_of(frontmatter: Option<&FrontmatterFields>, root: &Block) -> Option<String> {
    if let Some(title) = frontmatter
        .and_then(|f| f.title.as_deref())
        .map(str::trim)
        .filter(|t| !t.is_empty())
    {
        return Some(title.to_owned());
    }
    leading_h1(root).map(|block| liyasa_components::text::of(&block.children))
}

/// The body with the leading H1 removed when it only repeats the title, so a
/// page does not open with the same heading twice.
fn body_of(root: &Block, title: Option<&str>) -> Vec<Node> {
    let repeated = leading_h1(root).is_some_and(|block| {
        let text = liyasa_components::text::of(&block.children);
        title.is_some_and(|title| text.trim().eq_ignore_ascii_case(title.trim()))
    });
    if !repeated {
        return root.children.clone();
    }
    root.children
        .iter()
        .skip_while(|node| !matches!(node, Node::Block(b) if is_h1(b)))
        .skip(1)
        .cloned()
        .collect()
}

fn leading_h1(root: &Block) -> Option<&Block> {
    root.children.iter().find_map(|node| match node {
        Node::Block(block) if is_h1(block) => Some(block),
        _ => None,
    })
}

fn is_h1(block: &Block) -> bool {
    matches!(block.kind, BlockKind::Heading { level: 1, .. })
}

/// The site-wide and per-page instructions, in that order (CM-142).
fn agent_notes(site: Option<&str>, frontmatter: Option<&FrontmatterFields>) -> Vec<String> {
    let page = frontmatter.and_then(|f| match f.ai.as_ref() {
        Some(AiSetting::Options { instructions }) => instructions.as_deref(),
        _ => None,
    });
    [site, page]
        .into_iter()
        .flatten()
        .map(str::trim)
        .filter(|note| !note.is_empty())
        .map(str::to_owned)
        .collect()
}

#[cfg(test)]
mod tests;
