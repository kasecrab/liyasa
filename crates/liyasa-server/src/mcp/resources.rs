//! Resources: `llms.txt` and every page (MCP-01).
//!
//! A resource is addressed by URI, so the URI has to be one an agent can also
//! use outside MCP. Every resource here is named by its own public URL —
//! `https://docs.acme.com/guides/install.md` — rather than by an invented
//! scheme, so an agent that read a resource can put the same string in a
//! browser, a fetch, or a citation and land on the same bytes. A site that
//! declares no canonical origin has no such URL, and only then does the
//! `liyasa:` scheme appear.
//!
//! The listing is paginated. MCP-01 says "each page", and a site with five
//! thousand of them would otherwise answer `resources/list` with a single
//! response larger than most clients' message limit — which reads as the
//! server being broken rather than as the site being large.

use serde_json::{Value, json};

use super::reader::{Scope, SiteReader};

/// How many resources one `resources/list` page carries.
pub const PAGE_SIZE: usize = 200;

pub const MARKDOWN: &str = "text/markdown";
pub const PLAIN_TEXT: &str = "text/plain";

/// The path `llms.txt` is served at, and the suffix that makes a page URI
/// name its Markdown rather than its HTML.
pub const LLMS_TXT: &str = "/llms.txt";

/// The URI for a site path, carrying `build.basePath`.
///
/// The prefix is not decoration: on a site served under one, the page really
/// is at `<origin>/docs/guides/install.md`, and a URI without it names
/// nothing. `CanonicalOrigin::resource_url` builds every other published
/// address the same way.
pub fn uri_for(reader: &dyn SiteReader, path: &str) -> String {
    let site = reader.site();
    let path = match path.starts_with('/') {
        true => format!("{}{path}", site.base_path),
        false => format!("{}/{path}", site.base_path),
    };
    match site.origin.as_deref() {
        Some(origin) => format!("{}{path}", origin.trim_end_matches('/')),
        // Three slashes: an empty authority, which is what RFC 3986 asks for
        // when a hierarchical URI has a path and no host. `liyasa:/path` is
        // also legal and is the spelling clients get wrong.
        None => format!("liyasa://{path}"),
    }
}

/// The path part of a resource URI: authority, query and fragment dropped.
///
/// The authority is dropped rather than compared, the same way `fetch`
/// resolves a locator — a site served on two domains would refuse its own URL
/// if the host had to match.
fn path_of(uri: &str) -> &str {
    let rest = match uri.split_once("://") {
        Some((_, rest)) => match rest.find('/') {
            Some(at) => &rest[at..],
            None => "/",
        },
        None => uri,
    };
    rest.split(['?', '#']).next().unwrap_or(rest)
}

/// `resources/list`, from `cursor`.
pub fn list(reader: &dyn SiteReader, scope: &Scope, cursor: Option<&str>) -> Value {
    let mut all: Vec<Value> = Vec::new();
    if reader.llms_txt(scope).is_some() {
        all.push(json!({
            "uri": uri_for(reader, LLMS_TXT),
            "name": "llms.txt",
            "title": "Documentation index",
            "description": "The site's index for agents: every section and page, one line each.",
            "mimeType": PLAIN_TEXT
        }));
    }
    for page in reader.pages(scope) {
        all.push(json!({
            "uri": uri_for(reader, &markdown_path(&page.route)),
            "name": page.route,
            "title": page.title,
            "mimeType": MARKDOWN
        }));
    }

    let start = cursor
        .and_then(|cursor| cursor.parse::<usize>().ok())
        .unwrap_or(0)
        .min(all.len());
    let end = (start + PAGE_SIZE).min(all.len());
    let mut result = json!({ "resources": all[start..end].to_vec() });
    if end < all.len()
        && let Some(object) = result.as_object_mut()
    {
        object.insert("nextCursor".to_owned(), json!(end.to_string()));
    }
    result
}

/// `resources/read`.
///
/// Returns `None` when the URI names nothing here, which the caller turns
/// into the protocol's own "resource not found" rather than an empty read —
/// an empty `contents` array reads as a page that exists and is blank.
pub fn read(reader: &dyn SiteReader, scope: &Scope, uri: &str) -> Option<Value> {
    // The whole path, not a suffix: `ends_with("/llms.txt")` would serve this
    // site's index for `https://somewhere.else/docs/llms.txt`, which is a
    // different document with the same name.
    if path_of(uri) == format!("{}{LLMS_TXT}", reader.site().base_path) {
        let text = reader.llms_txt(scope)?;
        return Some(json!({
            "contents": [{ "uri": uri, "mimeType": PLAIN_TEXT, "text": text }]
        }));
    }
    // `page` takes a route, a `.md` twin or a URL, which is every spelling a
    // resource URI can have.
    let page = reader.page(uri, None, scope).ok()?;
    Some(json!({
        "contents": [{
            "uri": uri_for(reader, &markdown_path(&page.route)),
            "name": page.route,
            "title": page.title,
            "mimeType": MARKDOWN,
            "text": page.markdown
        }]
    }))
}

/// `/guides/install` names its Markdown at `/guides/install.md`; `/` names it
/// at `/index.md`.
fn markdown_path(route: &str) -> String {
    match route.trim_end_matches('/') {
        "" => "/index.md".to_owned(),
        route => format!("{route}.md"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::reader::{Hit, PageRef, PageText, SiteInfo, ToolFailure};

    struct Fake {
        info: SiteInfo,
        pages: Vec<PageRef>,
    }

    impl SiteReader for Fake {
        fn site(&self) -> &SiteInfo {
            &self.info
        }
        fn pages(&self, _scope: &Scope) -> Vec<PageRef> {
            self.pages.clone()
        }
        fn page(
            &self,
            locator: &str,
            _section: Option<&str>,
            _scope: &Scope,
        ) -> Result<PageText, ToolFailure> {
            let route = locator
                .rsplit_once("://")
                .map(|(_, rest)| match rest.find('/') {
                    Some(at) => rest[at..].to_owned(),
                    None => "/".to_owned(),
                })
                .unwrap_or_else(|| locator.to_owned());
            let route = route
                .strip_suffix("/index.md")
                .or_else(|| route.strip_suffix(".md"))
                .unwrap_or(&route);
            let route = match route {
                "" => "/",
                other => other,
            };
            self.pages
                .iter()
                .find(|page| page.route == route)
                .map(|page| PageText {
                    route: page.route.clone(),
                    title: page.title.clone(),
                    anchor: String::new(),
                    markdown: format!("# {}\n", page.title),
                })
                .ok_or_else(|| ToolFailure::NotFound(locator.to_owned()))
        }
        fn search(&self, _q: &str, _n: usize, _s: &Scope) -> Result<Vec<Hit>, ToolFailure> {
            Ok(Vec::new())
        }
        fn operation(&self, _o: &str, _s: &Scope) -> Result<PageText, ToolFailure> {
            Err(ToolFailure::Unavailable("none".to_owned()))
        }
        fn llms_txt(&self, _scope: &Scope) -> Option<String> {
            Some("# Acme docs\n".to_owned())
        }
    }

    fn site(origin: Option<&str>, count: usize) -> Fake {
        site_under(origin, "", count)
    }

    fn site_under(origin: Option<&str>, base_path: &str, count: usize) -> Fake {
        Fake {
            info: SiteInfo {
                name: "Acme docs".to_owned(),
                description: None,
                origin: origin.map(str::to_owned),
                base_path: base_path.to_owned(),
            },
            pages: (0..count)
                .map(|n| PageRef {
                    route: format!("/p{n}"),
                    title: format!("Page {n}"),
                    depth: 1,
                })
                .collect(),
        }
    }

    #[test]
    fn a_resource_uri_is_the_page_s_own_public_url() {
        // So an agent can cite it, fetch it, or open it without a translation
        // step that only this server knows.
        let reader = site(Some("https://docs.acme.com/"), 1);
        let listed = list(&reader, &Scope::anonymous(), None);
        let uris: Vec<&str> = listed["resources"]
            .as_array()
            .expect("resources")
            .iter()
            .filter_map(|entry| entry["uri"].as_str())
            .collect();
        assert_eq!(
            uris,
            [
                "https://docs.acme.com/llms.txt",
                "https://docs.acme.com/p0.md"
            ]
        );
    }

    #[test]
    fn a_site_with_no_origin_falls_back_to_a_scheme_of_our_own() {
        let reader = site(None, 1);
        assert_eq!(uri_for(&reader, "/p0.md"), "liyasa:///p0.md");
    }

    #[test]
    fn the_root_page_names_index_md_rather_than_a_bare_dot_md() {
        assert_eq!(markdown_path("/"), "/index.md");
        assert_eq!(markdown_path("/guides/install"), "/guides/install.md");
    }

    #[test]
    fn a_long_listing_is_paginated_and_the_cursor_reaches_the_end() {
        let reader = site(Some("https://docs.acme.com"), PAGE_SIZE + 5);
        let first = list(&reader, &Scope::anonymous(), None);
        assert_eq!(
            first["resources"].as_array().expect("array").len(),
            PAGE_SIZE
        );
        let cursor = first["nextCursor"].as_str().expect("a cursor");

        let second = list(&reader, &Scope::anonymous(), Some(cursor));
        // PAGE_SIZE + 5 pages plus llms.txt is PAGE_SIZE + 6 resources.
        assert_eq!(second["resources"].as_array().expect("array").len(), 6);
        assert!(
            second["nextCursor"].is_null(),
            "the last page must not offer another: {second}"
        );
    }

    #[test]
    fn a_cursor_past_the_end_is_an_empty_page_rather_than_a_panic() {
        let reader = site(Some("https://docs.acme.com"), 2);
        let listed = list(&reader, &Scope::anonymous(), Some("9999"));
        assert!(listed["resources"].as_array().expect("array").is_empty());
    }

    #[test]
    fn reading_a_page_uri_returns_its_markdown_and_reading_a_stranger_returns_nothing() {
        let reader = site(Some("https://docs.acme.com"), 2);
        let read =
            read(&reader, &Scope::anonymous(), "https://docs.acme.com/p1.md").expect("the page");
        assert_eq!(read["contents"][0]["text"], json!("# Page 1\n"));
        assert_eq!(read["contents"][0]["mimeType"], json!(MARKDOWN));

        assert!(
            super::read(
                &reader,
                &Scope::anonymous(),
                "https://elsewhere.example/x.md"
            )
            .is_none(),
            "a URI this site does not serve must not read as an empty page"
        );
    }

    #[test]
    fn llms_txt_reads_through_its_own_uri() {
        let reader = site(Some("https://docs.acme.com"), 1);
        let read = read(
            &reader,
            &Scope::anonymous(),
            "https://docs.acme.com/llms.txt",
        )
        .expect("the index");
        assert_eq!(read["contents"][0]["text"], json!("# Acme docs\n"));
    }

    #[test]
    fn a_different_path_ending_in_llms_txt_is_a_different_document() {
        // The match is on the whole path. A suffix match would serve this
        // site's index for `/vendor/llms.txt`, which is some vendor's index
        // that happens to share a file name.
        let reader = site(Some("https://docs.acme.com"), 1);
        assert!(
            read(
                &reader,
                &Scope::anonymous(),
                "https://docs.acme.com/vendor/llms.txt"
            )
            .is_none()
        );
    }

    #[test]
    fn the_authority_is_ignored_the_same_way_fetch_ignores_it() {
        // Deliberate, and asserted rather than left to be discovered: a site
        // served on two domains would refuse its own URL if the host had to
        // match, and `fetch` already resolves a locator this way. Nothing is
        // disclosed by it — `llms_txt` is filtered through the caller's scope
        // whichever spelling asked for it.
        let reader = site(Some("https://docs.acme.com"), 1);
        assert!(
            read(
                &reader,
                &Scope::anonymous(),
                "https://somewhere.else/llms.txt"
            )
            .is_some()
        );
    }

    #[test]
    fn a_site_served_under_a_prefix_names_its_resources_under_it() {
        // `CanonicalOrigin::resource_url` puts `build.basePath` into every
        // other published address, so a resource URI without it names nothing.
        let reader = site_under(Some("https://acme.example"), "/docs", 1);
        let listed = list(&reader, &Scope::anonymous(), None);
        let uris: Vec<&str> = listed["resources"]
            .as_array()
            .expect("resources")
            .iter()
            .filter_map(|entry| entry["uri"].as_str())
            .collect();
        assert_eq!(
            uris,
            [
                "https://acme.example/docs/llms.txt",
                "https://acme.example/docs/p0.md"
            ]
        );
        assert!(
            read(
                &reader,
                &Scope::anonymous(),
                "https://acme.example/docs/llms.txt"
            )
            .is_some()
        );
        // And the un-prefixed spelling is not this site's index either.
        assert!(
            read(
                &reader,
                &Scope::anonymous(),
                "https://acme.example/llms.txt"
            )
            .is_none()
        );
    }
}
