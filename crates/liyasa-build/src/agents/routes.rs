//! The Markdown routes (RX-60, spec check `markdown-url-support`).
//!
//! A page is reachable as Markdown at two paths, because agents guess both:
//! `<route>.md` and `<route>/index.md`. One body is written to both, so the
//! spec's parity check cannot fail on a difference between two spellings of
//! the same page.

use crate::agents::resource::{self, Resource, Surfaces};
use crate::agents::site::{PageRecord, SiteInput};

/// What a Markdown route is served as.
pub const MEDIA_TYPE: &str = resource::MARKDOWN;

/// The header a negotiated HTML route must carry, so a cache does not serve
/// one representation for the other.
pub const VARY: &str = "Accept";

/// The `Accept` value that asks the HTML route for Markdown instead.
pub const ACCEPT: &str = "text/markdown";

/// The two paths one page's Markdown is served from.
pub fn paths(page: &PageRecord) -> Vec<String> {
    let route = page.route.as_str().trim_end_matches('/');
    if route.is_empty() {
        return vec!["/index.md".to_owned()];
    }
    vec![format!("{route}.md"), format!("{route}/index.md")]
}

/// Every Markdown route of a site. A personalized page has none: its Markdown
/// would carry reader values into a shared surface (SRC-12).
pub fn generate(site: &SiteInput) -> Surfaces {
    let mut out = Surfaces::default();
    for page in site.published() {
        for path in paths(page) {
            out.resources
                .push(Resource::new(path, MEDIA_TYPE, page.markdown.clone()));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use liyasa_core::ids::{Locale, Route};

    use super::*;
    use crate::agents::site::{AgentsSettings, CanonicalOrigin, FeedsSettings};

    fn page(route: &str, title: &str) -> PageRecord {
        PageRecord {
            id: None,
            route: Route::new(route),
            title: title.to_owned(),
            description: None,
            locale: Locale::new("en"),
            version: None,
            tab: None,
            group: None,
            indexable: true,
            personalized: false,
            markdown: format!("# {title}\n\nBody of {title}.\n"),
            updated: None,
            changelog: false,
        }
    }

    fn site() -> SiteInput {
        SiteInput {
            name: "Liyasa".to_owned(),
            summary: None,
            origin: CanonicalOrigin::parse("https://example.com").expect("a valid origin"),
            locale: Locale::new("en"),
            version: None,
            pages: vec![page("/guide", "Guide"), page("/", "Home")],
            nav: Vec::new(),
            agents: AgentsSettings::default(),
            feeds: FeedsSettings::default(),
        }
    }

    #[test]
    fn rx_60_a_page_is_served_from_both_markdown_paths_with_one_body() {
        let surfaces = generate(&site());
        let direct = surfaces.get("/guide.md").expect("/guide.md");
        let indexed = surfaces.get("/guide/index.md").expect("/guide/index.md");
        assert_eq!(direct.body, indexed.body);
        assert_eq!(direct.media_type, "text/markdown; charset=utf-8");
        assert_eq!(indexed.media_type, direct.media_type);
    }

    #[test]
    fn rx_60_the_site_root_has_one_markdown_path() {
        assert_eq!(paths(&page("/", "Home")), ["/index.md"]);
    }

    #[test]
    fn rx_60_the_negotiated_route_declares_what_it_varies_on() {
        assert_eq!(VARY, "Accept");
        assert_eq!(ACCEPT, "text/markdown");
    }

    #[test]
    fn rx_60_a_personalized_page_has_no_markdown_route() {
        let mut site = site();
        let mut private = page("/dashboard", "Dashboard");
        private.personalized = true;
        site.pages.push(private);
        let surfaces = generate(&site);
        assert!(surfaces.get("/dashboard.md").is_none());
        assert!(surfaces.get("/dashboard/index.md").is_none());
    }

    #[test]
    fn rx_60_a_noindex_page_has_no_markdown_route() {
        let mut site = site();
        let mut hidden = page("/internal", "Internal");
        hidden.indexable = false;
        site.pages.push(hidden);
        assert!(generate(&site).get("/internal.md").is_none());
    }
}
