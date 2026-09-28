//! [`SiteReader`] over a built bundle.
//!
//! One implementation serves both hosts MCP-01 and MCP-04 name: `liyasa
//! serve` hands it the bundle it is already serving, and `liyasa mcp --dist`
//! opens the same `dist/` directory and hands it that. The tools cannot tell
//! the difference, which is the point — an OSS user running the CLI and a
//! reader's agent hitting the hosted site get the same answers.
//!
//! ## Why the Markdown is cached
//!
//! `search` here is a keyword scan, not an index: `liyasa-search`'s tantivy
//! engine is behind that crate's `server` feature and `/_liyasa/search` does
//! not exist yet either (defect 146), so an MCP `search` that waited for it
//! would ship nothing. Scanning means reading every page the caller may see,
//! and doing that from the filesystem per request would put hundreds of
//! blocking reads inside an async handler. So the twins are read once, at
//! construction, up to [`CACHE_BUDGET`]; past that the scan falls back to
//! reading on demand, because a site large enough to exceed it is a site
//! where holding the whole corpus resident is the worse failure.

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::sync::Arc;

use crate::auth::groups::{self, Decision, Declared, SiteDefault};
use crate::routes::bundle::Bundle;

use super::markdown::{self, Section};
use super::reader::{Hit, PageRef, PageText, Scope, SiteInfo, SiteReader, ToolFailure};

/// How much page text is held resident. Beyond it the scan reads from disk.
pub const CACHE_BUDGET: usize = 48 * 1024 * 1024;

/// How many results a `search` returns when the caller names no limit.
pub const DEFAULT_LIMIT: usize = 10;

/// The ceiling on `limit`, so one call cannot ask for the whole corpus as
/// search results and bypass every listing filter's cost.
pub const MAX_LIMIT: usize = 50;

struct Page {
    route: String,
    title: String,
    /// The twin's path inside the bundle.
    path: String,
    /// `None` once [`CACHE_BUDGET`] is spent.
    text: Option<String>,
    access: Vec<Declared>,
}

pub struct BundleReader {
    bundle: Arc<Bundle>,
    info: SiteInfo,
    pages: Vec<Page>,
    by_route: BTreeMap<String, usize>,
}

impl BundleReader {
    pub fn new(bundle: Arc<Bundle>, info: SiteInfo) -> Self {
        let mut pages = Vec::new();
        let mut spent = 0usize;
        for entry in &bundle.manifest().routes {
            if entry.hidden {
                continue;
            }
            let route = entry.route.as_str().to_owned();
            let text = bundle
                .read(&entry.markdown)
                .ok()
                .and_then(|bytes| String::from_utf8(bytes).ok());
            let title = match &text {
                Some(text) => markdown::title_of(text, &route),
                // A dynamic page has no twin on disk (§6.6.4). It is still a
                // route an agent may fetch, so it is listed under the name
                // its route gives it rather than dropped.
                None => markdown::title_of("", &route),
            };
            let text = match text {
                Some(text) if spent + text.len() <= CACHE_BUDGET => {
                    spent += text.len();
                    Some(text)
                }
                _ => None,
            };
            pages.push(Page {
                access: bundle.access_chain(&route),
                route,
                title,
                path: entry.markdown.clone(),
                text,
            });
        }
        pages.sort_by(|a, b| a.route.cmp(&b.route));
        let by_route = pages
            .iter()
            .enumerate()
            .map(|(index, page)| (page.route.clone(), index))
            .collect();
        Self {
            bundle,
            info,
            pages,
            by_route,
        }
    }

    fn decide(&self, page: &Page, scope: &Scope) -> Decision {
        groups::decide(scope.site, &page.access, scope.reader())
    }

    fn body<'a>(&self, page: &'a Page) -> Option<Cow<'a, str>> {
        match &page.text {
            Some(text) => Some(Cow::Borrowed(text.as_str())),
            None => self
                .bundle
                .read(&page.path)
                .ok()
                .and_then(|bytes| String::from_utf8(bytes).ok())
                .map(Cow::Owned),
        }
    }

    /// A locator as MCP-01 allows it — a route, a Markdown twin, or an
    /// absolute URL on this site — reduced to a route this bundle knows.
    fn route_of(&self, locator: &str) -> Option<&Page> {
        let mut path = locator.trim();
        if let Some(rest) = path.split_once("://").map(|(_, rest)| rest) {
            // Drop the authority. Whether the host matches is not this
            // function's question: a site served on two domains would refuse
            // its own URL if it were.
            path = match rest.find('/') {
                Some(at) => &rest[at..],
                None => "/",
            };
        }
        let path = path.split(['?', '#']).next().unwrap_or(path);
        let path = self
            .bundle
            .strip_base(path)
            .unwrap_or_else(|| path.to_owned());
        let path = path
            .strip_suffix("/index.md")
            .or_else(|| path.strip_suffix(".md"))
            .unwrap_or(&path);
        let normalized = match path.trim_end_matches('/') {
            "" => "/".to_owned(),
            trimmed => trimmed.to_owned(),
        };
        let index = self.by_route.get(&normalized)?;
        self.pages.get(*index)
    }

    /// The pages this caller may see, as indices into `self.pages`.
    fn visible<'a>(&'a self, scope: &'a Scope) -> impl Iterator<Item = &'a Page> + 'a {
        self.pages
            .iter()
            .filter(move |page| self.decide(page, scope).is_allowed())
    }

    /// Says the same thing the HTML route at the same URL says, which is what
    /// `routes::sign_in` decides and why the two branches differ.
    ///
    /// On a PUBLIC site a restricted page must be indistinguishable from one
    /// that does not exist, so `SignIn` is a miss: naming it would confirm
    /// the page is there, and an agent is a much better enumerator of routes
    /// than a person. On a PRIVATE site the site is known private and there
    /// is nothing to conceal, so the agent is told a session would settle it
    /// — otherwise an agent whose user could sign in has no way to find that
    /// out.
    fn refuse(&self, page: &Page, scope: &Scope) -> Option<ToolFailure> {
        let missing = || ToolFailure::NotFound(format!("no page at `{}`", page.route));
        match self.decide(page, scope) {
            Decision::Allow => None,
            Decision::SignIn if scope.site == SiteDefault::Private => {
                Some(ToolFailure::Unavailable(format!(
                    "`{}` needs a signed-in reader and this connection has no session",
                    page.route
                )))
            }
            Decision::SignIn | Decision::Deny => Some(missing()),
        }
    }
}

impl SiteReader for BundleReader {
    fn site(&self) -> &SiteInfo {
        &self.info
    }

    fn pages(&self, scope: &Scope) -> Vec<PageRef> {
        self.visible(scope)
            .map(|page| PageRef {
                depth: depth_of(&page.route),
                route: page.route.clone(),
                title: page.title.clone(),
            })
            .collect()
    }

    fn page(
        &self,
        locator: &str,
        section: Option<&str>,
        scope: &Scope,
    ) -> Result<PageText, ToolFailure> {
        if locator.trim().is_empty() {
            return Err(ToolFailure::BadInput(
                "`route` is required; it is a route like `/guides/install`, that page's \
                 `.md` twin, or its absolute URL"
                    .to_owned(),
            ));
        }
        let page = self.route_of(locator).ok_or_else(|| {
            ToolFailure::NotFound(format!(
                "no page at `{locator}`; `list_pages` returns every route this site has"
            ))
        })?;
        if let Some(refusal) = self.refuse(page, scope) {
            return Err(refusal);
        }
        let body = self.body(page).ok_or_else(|| {
            ToolFailure::Unavailable(format!(
                "`{}` is rendered per request and has no Markdown in this build",
                page.route
            ))
        })?;
        let Some(section) = section.filter(|name| !name.trim().is_empty()) else {
            return Ok(PageText {
                route: page.route.clone(),
                title: page.title.clone(),
                anchor: String::new(),
                markdown: body.into_owned(),
            });
        };
        let found = markdown::sections(&body);
        let wanted = markdown::section_named(&found, section).ok_or_else(|| {
            ToolFailure::NotFound(format!(
                "`{}` has no section `{section}`; it has {}",
                page.route,
                list_anchors(&found)
            ))
        })?;
        Ok(PageText {
            route: page.route.clone(),
            title: page.title.clone(),
            anchor: wanted.anchor.clone(),
            markdown: wanted.body.clone(),
        })
    }

    fn search(&self, query: &str, limit: usize, scope: &Scope) -> Result<Vec<Hit>, ToolFailure> {
        let terms = markdown::terms(query);
        if terms.is_empty() {
            return Err(ToolFailure::BadInput(format!(
                "`{query}` has no word longer than one character to search for"
            )));
        }
        let limit = limit.clamp(1, MAX_LIMIT);
        let mut scored: Vec<(u32, Hit)> = Vec::new();
        for page in self.visible(scope) {
            let Some(body) = self.body(page) else {
                continue;
            };
            let Some((score, hit)) = score_page(page, &body, &terms) else {
                continue;
            };
            scored.push((score, hit));
        }
        // Score first, then route, so two pages that match equally come back
        // in a stable order rather than in filesystem order.
        scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.route.cmp(&b.1.route)));
        Ok(scored.into_iter().take(limit).map(|(_, hit)| hit).collect())
    }

    fn operation(&self, operation: &str, scope: &Scope) -> Result<PageText, ToolFailure> {
        super::openapi::find(&self.bundle, operation, scope)
    }

    fn llms_txt(&self, scope: &Scope) -> Option<String> {
        let body = self.bundle.read("llms.txt").ok()?;
        let entries = self.bundle.listing_entries("llms.txt");
        if entries.is_empty() {
            return String::from_utf8(body).ok();
        }
        let allowed = |route: &str| {
            groups::decide(scope.site, &self.bundle.access_chain(route), scope.reader())
                == Decision::Allow
        };
        String::from_utf8(crate::routes::site::filter_listing(
            &body, entries, &allowed,
        ))
        .ok()
    }
}

/// `/` is 0, `/guides` is 1, `/guides/install` is 2.
fn depth_of(route: &str) -> u8 {
    route
        .split('/')
        .filter(|part| !part.is_empty())
        .count()
        .min(u8::MAX as usize) as u8
}

fn list_anchors(sections: &[Section]) -> String {
    match sections.is_empty() {
        true => "none".to_owned(),
        false => sections
            .iter()
            .map(|section| format!("`{}`", section.anchor))
            .collect::<Vec<_>>()
            .join(", "),
    }
}

/// How well one page answers the query, and the best place in it to point at.
///
/// A page matching every term outranks one matching many occurrences of a
/// single term, because "install docker" should find the page about both and
/// not the page that says "install" forty times.
fn score_page(page: &Page, body: &str, terms: &[String]) -> Option<(u32, Hit)> {
    let body_lower = body.to_lowercase();
    let title_lower = page.title.to_lowercase();
    let route_lower = page.route.to_lowercase();

    let mut matched = 0u32;
    let mut occurrences = 0u32;
    let mut in_title = 0u32;
    for term in terms {
        let count = body_lower.matches(term.as_str()).count() as u32;
        if count > 0 {
            matched += 1;
            occurrences += count.min(20);
        }
        if title_lower.contains(term.as_str()) || route_lower.contains(term.as_str()) {
            in_title += 1;
            if count == 0 {
                matched += 1;
            }
        }
    }
    if matched == 0 {
        return None;
    }
    let all = u32::from(matched as usize == terms.len());
    let score = matched * 100 + all * 500 + in_title * 50 + occurrences;

    // The DEEPEST section holding the match, not the first. A section runs to
    // the next heading of its level or shallower, so a page's own `# Title`
    // contains every word on the page and finding it first would anchor every
    // result at the top of the page — which is the same as no anchor at all,
    // and sends the agent back to re-read what it just searched.
    let sections = markdown::sections(body);
    let (section, anchor, text) = sections
        .iter()
        .filter(|section| {
            let lower = section.body.to_lowercase();
            terms.iter().any(|term| lower.contains(term.as_str()))
        })
        // `min_by_key` on the reversed level rather than `max_by_key`: the
        // latter returns the LAST of several equal maxima, so two sibling
        // sections both holding the term would anchor at the second one.
        .min_by_key(|section| std::cmp::Reverse(section.level))
        .map(|section| {
            (
                section.heading.clone(),
                section.anchor.clone(),
                section.body.as_str(),
            )
        })
        .unwrap_or_else(|| (String::new(), String::new(), body));

    Some((
        score,
        Hit {
            route: page.route.clone(),
            title: page.title.clone(),
            section,
            anchor,
            snippet: markdown::snippet_for(text, terms, 220)
                .or_else(|| markdown::snippet_for(body, terms, 220))
                .unwrap_or_default(),
        },
    ))
}
