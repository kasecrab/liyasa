//! The 404 page (RX-82, spec check `http-status-codes`).
//!
//! The check the spec runs is about the status line, which is the server's to
//! send. What the build owes it is a body to send with it, at a path that is
//! not itself a page: a 404 that appears in the index, the sitemap, or the
//! route table is a fabricated URL that returns 200, which is the failure the
//! check exists to catch.

use std::fmt::Write as _;

use crate::agents::llms::ROOT_PATH;
use crate::agents::resource::{self, Resource, Surfaces};
use crate::agents::site::SiteInput;

/// The static host's 404 document, by convention.
pub const HTML_PATH: &str = "/404.html";

/// The Markdown form, for an agent that asked for `text/markdown`.
pub const MARKDOWN_PATH: &str = "/404.md";

/// The status every one of these bodies is served with. Never 200.
pub const STATUS: u16 = 404;

/// Generates the 404 body from `errors/404.md` or a default.
///
/// `custom` is the repository's `errors/404.md`, already rendered to agent
/// Markdown, or the body `errors.notFound` names in config.
pub fn generate(site: &SiteInput, custom: Option<&str>) -> Surfaces {
    let markdown = custom
        .map(str::trim)
        .filter(|body| !body.is_empty())
        .map_or_else(|| default_body(site), str::to_owned);
    Surfaces {
        resources: vec![
            Resource::new(MARKDOWN_PATH, resource::MARKDOWN, markdown.clone()),
            Resource::new(HTML_PATH, "text/html; charset=utf-8", html(site, &markdown)),
        ],
        diagnostics: Default::default(),
    }
}

/// A 404 an agent can act on: what happened, and the two URLs that lead back to
/// real content.
fn default_body(site: &SiteInput) -> String {
    let mut out = String::from("# Page not found\n\n");
    let _ = writeln!(
        out,
        "This URL does not exist on {}. It was never published, or it moved \
         without a redirect.\n",
        site.name
    );
    let _ = writeln!(
        out,
        "- The documentation index is at {}",
        site.origin.resource_url(ROOT_PATH)
    );
    let _ = writeln!(
        out,
        "- The site root is at {}",
        site.origin.page_url(&liyasa_core::ids::Route::new("/"))
    );
    out
}

/// The minimum a static host needs: the same words, as a document a browser can
/// render, with no navigation and no scripts.
fn html(site: &SiteInput, markdown: &str) -> String {
    let title = markdown
        .lines()
        .find_map(|line| line.strip_prefix("# "))
        .unwrap_or("Page not found");
    format!(
        "<!doctype html>\n\
         <html lang=\"{locale}\">\n\
         <head>\n\
         <meta charset=\"utf-8\">\n\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
         <meta name=\"robots\" content=\"noindex\">\n\
         <title>{title} — {name}</title>\n\
         </head>\n\
         <body>\n\
         <main>\n\
         <h1>{title}</h1>\n\
         <p>This URL does not exist on {name}.</p>\n\
         <p><a href=\"{root}\">Go to the documentation</a></p>\n\
         </main>\n\
         </body>\n\
         </html>\n",
        locale = site.locale,
        name = escape(&site.name),
        title = escape(title),
        root = site.origin.page_url(&liyasa_core::ids::Route::new("/")),
    )
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use liyasa_core::ids::{Locale, Route};

    use super::*;
    use crate::agents::llms;
    use crate::agents::site::{AgentsSettings, CanonicalOrigin, FeedsSettings, PageRecord};

    fn site() -> SiteInput {
        SiteInput {
            name: "Liyasa".to_owned(),
            summary: Some("Documentation.".to_owned()),
            origin: CanonicalOrigin::parse("https://example.com").expect("a valid origin"),
            locale: Locale::new("en"),
            version: None,
            pages: vec![PageRecord {
                id: None,
                route: Route::new("/guide/install"),
                title: "Install".to_owned(),
                description: None,
                locale: Locale::new("en"),
                version: None,
                tab: None,
                group: None,
                indexable: true,
                personalized: false,
                markdown: "# Install\n".to_owned(),
                updated: None,
                changelog: false,
            }],
            nav: Vec::new(),
            agents: AgentsSettings::default(),
            feeds: FeedsSettings::default(),
        }
    }

    #[test]
    fn rx_82_the_default_body_leads_back_to_the_index() {
        let surfaces = generate(&site(), None);
        let body = &surfaces.get(MARKDOWN_PATH).expect("the markdown 404").body;
        assert!(body.starts_with("# Page not found"), "{body}");
        assert!(body.contains("https://example.com/llms.txt"), "{body}");
    }

    #[test]
    fn rx_82_a_custom_body_replaces_the_default() {
        let surfaces = generate(&site(), Some("# Gone\n\nTry the search.\n"));
        let body = &surfaces.get(MARKDOWN_PATH).expect("the markdown 404").body;
        assert_eq!(body, "# Gone\n\nTry the search.");
        let html = &surfaces.get(HTML_PATH).expect("the html 404").body;
        assert!(html.contains("<h1>Gone</h1>"), "{html}");
    }

    #[test]
    fn rx_82_an_empty_custom_body_falls_back_to_the_default() {
        let surfaces = generate(&site(), Some("   \n"));
        assert!(
            surfaces
                .get(MARKDOWN_PATH)
                .expect("the markdown 404")
                .body
                .starts_with("# Page not found")
        );
    }

    #[test]
    fn rx_82_the_html_form_is_noindex() {
        let surfaces = generate(&site(), None);
        let html = &surfaces.get(HTML_PATH).expect("the html 404").body;
        assert!(
            html.contains(r#"<meta name="robots" content="noindex">"#),
            "{html}"
        );
    }

    #[test]
    fn rx_82_the_404_is_never_a_page_the_index_lists() {
        let site = site();
        let index = llms::generate(&site);
        let root = &index.get(llms::ROOT_PATH).expect("an index").body;
        assert!(!root.contains("/404"), "{root}");
        for path in generate(&site, None).paths() {
            assert!(
                !site.pages.iter().any(|p| p.route.as_str() == path),
                "{path}"
            );
        }
    }

    #[test]
    fn rx_82_the_status_is_not_two_hundred() {
        assert_eq!(STATUS, 404);
    }
}
