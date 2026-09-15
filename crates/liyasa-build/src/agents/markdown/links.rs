//! Absolute links in Markdown output (RX-65, spec check
//! `markdown-link-portability`).
//!
//! Agent pipelines summarize and chunk a page until the base URL is gone, so a
//! link that was relative when it was served resolves against nothing by the
//! time it is followed. Every destination is therefore rewritten against
//! `seo.canonicalOrigin`, and one `W0406` is raised per destination that needed
//! it.

use std::collections::BTreeSet;

use liyasa_core::diagnostics::{Diagnostic, Diagnostics, code};
use liyasa_core::document::{Block, Inline, Node};
use liyasa_core::ids::Route;

use crate::agents::site::CanonicalOrigin;

/// What a destination is resolved against.
pub struct Links<'a> {
    pub origin: &'a CanonicalOrigin,
    /// The page the destination was written on.
    pub route: &'a Route,
    /// Routes the site publishes, so an internal link can take its `.md` form.
    pub routes: &'a BTreeSet<Route>,
}

impl Links<'_> {
    /// The absolute form of a destination, or `None` when it already is one.
    ///
    /// `mailto:`, `tel:` and every other scheme are left alone; an empty
    /// destination is left alone because there is nothing to resolve.
    pub fn absolute(&self, href: &str) -> Option<String> {
        let href = href.trim();
        if href.is_empty() || is_absolute(href) {
            return None;
        }
        if let Some(rest) = href.strip_prefix("//") {
            // Protocol-relative: an agent that lost the base URL lost the
            // scheme with it.
            return Some(format!("https://{rest}"));
        }
        let (path, tail) = split_tail(href);
        if path.is_empty() {
            // A bare fragment or query belongs to the page it was written on.
            return Some(format!("{}{tail}", self.origin.page_url(self.route)));
        }
        let resolved = if let Some(rooted) = path.strip_prefix('/') {
            normalize("", rooted)
        } else {
            normalize(parent_of(self.route.as_str()), path)
        };
        Some(format!("{}{tail}", self.target_url(&resolved)))
    }

    /// The URL an agent should follow for a site-relative path: the Markdown
    /// route when the path is a page, the path itself otherwise.
    fn target_url(&self, path: &str) -> String {
        let page = path.strip_suffix(".md").unwrap_or(path);
        let page = page.strip_suffix("/index").unwrap_or(page);
        let route = Route::new(if page.is_empty() { "/" } else { page });
        if self.routes.contains(&route) {
            return self.origin.markdown_url(&route);
        }
        self.origin.resource_url(path)
    }
}

/// Rewrites every link and image destination in a tree, reporting one `W0406`
/// per destination that was not already absolute.
pub fn absolutize(block: &mut Block, links: &Links<'_>, out: &mut Diagnostics) {
    let span = block.origin.span;
    for child in &mut block.children {
        match child {
            Node::Block(inner) => absolutize(inner, links, out),
            Node::Inline(inline) => absolutize_inline(inline, links, span, out),
        }
    }
}

fn absolutize_inline(
    inline: &mut Inline,
    links: &Links<'_>,
    span: Option<liyasa_core::span::Span>,
    out: &mut Diagnostics,
) {
    match inline {
        Inline::Link { href, children, .. } => {
            rewrite(href, links, span, out);
            for child in children {
                absolutize_inline(child, links, span, out);
            }
        }
        Inline::Image { src, dark, .. } => {
            rewrite(src, links, span, out);
            if let Some(dark) = dark {
                rewrite(dark, links, span, out);
            }
        }
        Inline::Emph(children) | Inline::Strong(children) | Inline::Strike(children) => {
            for child in children {
                absolutize_inline(child, links, span, out);
            }
        }
        Inline::InlineComponent { children, .. } => {
            for child in children {
                absolutize_inline(child, links, span, out);
            }
        }
        Inline::Text(_)
        | Inline::Code(_)
        | Inline::HtmlInline(_)
        | Inline::FootnoteRef(_)
        | Inline::SoftBreak
        | Inline::HardBreak
        | Inline::Math(_)
        | Inline::TemplateInline { .. } => {}
    }
}

fn rewrite(
    href: &mut String,
    links: &Links<'_>,
    span: Option<liyasa_core::span::Span>,
    out: &mut Diagnostics,
) {
    let Some(absolute) = links.absolute(href) else {
        return;
    };
    let mut diagnostic = Diagnostic::new(
        code::W0406,
        format!("`{href}` is not an absolute URL; agent pipelines lose the base URL"),
    )
    .help(format!("served as `{absolute}`"));
    if let Some(span) = span {
        diagnostic = diagnostic.at(span);
    }
    out.push(diagnostic);
    *href = absolute;
}

fn is_absolute(href: &str) -> bool {
    let scheme_len = href
        .find(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.')))
        .unwrap_or(0);
    scheme_len > 0
        && href.as_bytes()[0].is_ascii_alphabetic()
        && href[scheme_len..].starts_with(':')
}

/// Splits a destination into its path and everything from the first `?` or `#`.
fn split_tail(href: &str) -> (&str, &str) {
    match href.find(['?', '#']) {
        Some(at) => href.split_at(at),
        None => (href, ""),
    }
}

/// The directory a route's children are relative to: `/a/b` gives `/a`.
fn parent_of(route: &str) -> &str {
    let trimmed = route.trim_end_matches('/');
    match trimmed.rfind('/') {
        Some(at) => &trimmed[..at],
        None => "",
    }
}

/// Applies a relative path to a base directory, resolving `.` and `..`.
fn normalize(base: &str, path: &str) -> String {
    let mut parts: Vec<&str> = base.split('/').filter(|p| !p.is_empty()).collect();
    for segment in path.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            other => parts.push(other),
        }
    }
    let joined = parts.join("/");
    let trailing = if path.ends_with('/') && !joined.is_empty() {
        "/"
    } else {
        ""
    };
    format!("/{joined}{trailing}")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The fixture: `https://example.com/docs` serving `/guide/install`.
    struct Fixture {
        origin: CanonicalOrigin,
        route: Route,
        routes: BTreeSet<Route>,
    }

    impl Fixture {
        fn new() -> Self {
            Self {
                origin: CanonicalOrigin::parse("https://example.com/docs").expect("a valid origin"),
                route: Route::new("/guide/install"),
                routes: ["/guide/install", "/guide/faq", "/"]
                    .into_iter()
                    .map(Route::new)
                    .collect(),
            }
        }

        fn links(&self) -> Links<'_> {
            Links {
                origin: &self.origin,
                route: &self.route,
                routes: &self.routes,
            }
        }
    }

    #[test]
    fn rx_65_a_root_relative_link_becomes_absolute() {
        let fixture = Fixture::new();
        assert_eq!(
            fixture.links().absolute("/guide/faq"),
            Some("https://example.com/docs/guide/faq.md".to_owned())
        );
    }

    #[test]
    fn rx_65_a_relative_link_resolves_against_the_page() {
        let fixture = Fixture::new();
        let links = fixture.links();
        assert_eq!(
            links.absolute("faq"),
            Some("https://example.com/docs/guide/faq.md".to_owned())
        );
        assert_eq!(
            links.absolute("../guide/faq#offline"),
            Some("https://example.com/docs/guide/faq.md#offline".to_owned())
        );
    }

    #[test]
    fn rx_65_a_link_to_a_non_page_keeps_its_path() {
        let fixture = Fixture::new();
        assert_eq!(
            fixture.links().absolute("/assets/logo.png"),
            Some("https://example.com/docs/assets/logo.png".to_owned())
        );
    }

    #[test]
    fn rx_65_an_absolute_link_is_left_alone() {
        let fixture = Fixture::new();
        let links = fixture.links();
        assert_eq!(links.absolute("https://example.org/x"), None);
        assert_eq!(links.absolute("mailto:docs@example.com"), None);
        assert_eq!(links.absolute(""), None);
    }

    #[test]
    fn rx_65_a_bare_fragment_points_at_its_own_page() {
        let fixture = Fixture::new();
        assert_eq!(
            fixture.links().absolute("#requirements"),
            Some("https://example.com/docs/guide/install#requirements".to_owned())
        );
    }

    #[test]
    fn rx_65_a_protocol_relative_link_gains_a_scheme() {
        let fixture = Fixture::new();
        assert_eq!(
            fixture.links().absolute("//cdn.example.com/x.png"),
            Some("https://cdn.example.com/x.png".to_owned())
        );
    }

    #[test]
    fn rx_65_an_index_link_resolves_to_the_site_root() {
        let fixture = Fixture::new();
        assert_eq!(
            fixture.links().absolute("/index.md"),
            Some("https://example.com/docs/index.md".to_owned())
        );
    }
}
