//! `liyasa test --agents` against a build (SPEC-01, static half).
//!
//! Every check reads the artifacts the build produced and the headers the host
//! is configured to send. Nothing here fetches: the live half needs a server,
//! and a check that cannot be answered from the build says so rather than
//! guessing.

use std::collections::BTreeSet;

use liyasa_core::ids::Route;

use super::checks::CHECKS;
use super::options::Options;
use super::report::Report;
use super::score::{self, CheckResult, Outcome, RunFacts};
use crate::agents::continuation;
use crate::agents::llms::{self, ROOT_PATH};
use crate::agents::markdown::discovery_directive;
use crate::agents::notfound;
use crate::agents::resource::Surfaces;
use crate::agents::site::SiteInput;
use crate::agents::size;

/// One page as the build left it.
#[derive(Debug, Clone, PartialEq)]
pub struct BuiltPage {
    pub route: Route,
    /// The `.md` artifact, absent when no Markdown route was written.
    pub markdown: Option<String>,
    pub html: String,
    /// The response body a host sends, after decompression.
    pub transfer_bytes: u64,
    /// Behind an authentication gate.
    pub gated: bool,
    /// Server-rendered rather than an empty shell a script fills in.
    pub server_rendered: bool,
    /// Rendered, but with little content in it; counts as half (§25.1).
    pub sparse: bool,
    /// What `agents::size` measured, when the build measured it.
    pub size: Option<size::Report>,
}

impl BuiltPage {
    pub fn new(route: Route, markdown: String, html: String) -> Self {
        let transfer_bytes = html.len() as u64;
        Self {
            route,
            markdown: Some(markdown),
            html,
            transfer_bytes,
            gated: false,
            server_rendered: true,
            sparse: false,
            size: None,
        }
    }
}

/// What a static host is configured to send. A check about a header is about
/// the host's configuration, so the configuration is an input rather than an
/// assumption.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostHeaders {
    /// The `Content-Type` of a `.md` route.
    pub markdown_content_type: Option<String>,
    /// `Accept: text/markdown` on the HTML route returns Markdown.
    pub honours_accept: bool,
    pub vary_accept: bool,
    pub cache_control: Option<String>,
    pub etag: bool,
    pub last_modified: bool,
    /// The status an unknown route gets.
    pub not_found_status: u16,
    /// The status a redirect gets, when the site has any.
    pub redirect_status: Option<u16>,
    /// The site emits a `<meta refresh>` or a script redirect somewhere.
    pub javascript_redirects: bool,
    /// The host answers some requests with a challenge interstitial.
    pub serves_challenge: bool,
}

impl Default for HostHeaders {
    fn default() -> Self {
        Self {
            markdown_content_type: Some(crate::agents::resource::MARKDOWN.to_owned()),
            honours_accept: false,
            vary_accept: false,
            cache_control: Some("public, max-age=300, must-revalidate".to_owned()),
            etag: true,
            last_modified: true,
            not_found_status: notfound::STATUS,
            redirect_status: None,
            javascript_redirects: false,
            serves_challenge: false,
        }
    }
}

impl HostHeaders {
    /// What `liyasa serve` sends, which is the configuration the live checks
    /// are written against (RX-60, RX-13).
    pub fn server() -> Self {
        Self {
            honours_accept: true,
            vary_accept: true,
            redirect_status: Some(301),
            ..Self::default()
        }
    }
}

/// The build under test.
pub struct Built<'a> {
    pub site: &'a SiteInput,
    pub surfaces: &'a Surfaces,
    pub pages: &'a [BuiltPage],
    pub headers: HostHeaders,
}

/// Runs the static half of the check set and scores it.
pub fn run(built: &Built<'_>, options: &Options) -> Report {
    let results = evaluate(built, options);
    let facts = facts(built, options);
    let resolved = score::resolve(&results, &facts);
    let score = score::score(&resolved, &facts);
    Report::new(
        resolved,
        score,
        facts,
        Some(built.site.agents.spec_version.clone()),
    )
}

fn facts(built: &Built<'_>, options: &Options) -> RunFacts {
    let total = built.pages.len();
    let rendered: f64 = built
        .pages
        .iter()
        .map(|page| {
            if !page.server_rendered {
                0.0
            } else if page.sparse {
                0.5
            } else {
                1.0
            }
        })
        .sum();
    let gated = built.pages.iter().filter(|page| page.gated).count();
    let index = built.surfaces.get(ROOT_PATH);
    let cross_origin = index.map_or(0, |index| {
        llms::link_targets(&index.body)
            .into_iter()
            .filter(|href| href.starts_with("http") && !built.site.origin.contains(href))
            .count()
    });
    let readable = built
        .pages
        .iter()
        .any(|page| page.markdown.is_some() || (page.server_rendered && !page.sparse));

    RunFacts {
        pages_discovered: total,
        pages_selected: options.pages_selected(),
        rendering_proportion: if total == 0 {
            0.0
        } else {
            rendered / total as f64
        },
        gated_proportion: if total == 0 {
            0.0
        } else {
            gated as f64 / total as f64
        },
        no_viable_path: total > 0 && !readable,
        fetch_failure_rate: 0.0,
        cross_origin_llms_links: cross_origin,
    }
}

/// `Pass` when every page satisfied the check, `Partial` otherwise, and
/// `NotApplicable` when there was nothing to judge.
fn proportion(passed: usize, total: usize, detail: impl Into<String>) -> (Outcome, String) {
    let detail = detail.into();
    if total == 0 {
        return (
            Outcome::NotApplicable {
                reason: detail.clone(),
            },
            detail,
        );
    }
    if passed == total {
        (Outcome::Pass, detail)
    } else {
        (Outcome::Partial { passed, total }, detail)
    }
}

fn verdict(passed: bool, yes: impl Into<String>, no: impl Into<String>) -> (Outcome, String) {
    if passed {
        (Outcome::Pass, yes.into())
    } else {
        (Outcome::Fail, no.into())
    }
}

fn evaluate(built: &Built<'_>, options: &Options) -> Vec<CheckResult> {
    let mut out = Vec::with_capacity(CHECKS.len());
    let mut push = |id: &'static str, (outcome, detail): (Outcome, String)| {
        out.push(CheckResult::new(id, outcome, detail));
    };

    let index = built.surfaces.get(ROOT_PATH).map(|r| r.body.as_str());
    let thresholds = &options.thresholds;

    // ---- Content Discoverability ----
    push(
        "llms-txt-exists",
        verdict(
            index.is_some_and(|body| !body.trim().is_empty()),
            format!("`{ROOT_PATH}` is published"),
            format!("no `{ROOT_PATH}`"),
        ),
    );
    push("llms-txt-valid", llms_txt_valid(index));
    push("llms-txt-size", {
        let chars = index.map_or(0, |body| body.chars().count());
        verdict(
            chars <= thresholds.llms_txt_chars,
            format!("{chars} characters"),
            format!("{chars} characters, over {}", thresholds.llms_txt_chars),
        )
    });
    push(
        "llms-txt-links-resolve",
        links_resolve(built, index, options),
    );
    push(
        "llms-txt-links-markdown",
        links_markdown(built, index, options),
    );
    push("llms-txt-directive-html", directive_html(built));
    push("llms-txt-directive-md", directive_md(built));

    // ---- Markdown Availability ----
    let with_markdown = built
        .pages
        .iter()
        .filter(|page| page.markdown.is_some())
        .count();
    push(
        "markdown-url-support",
        proportion(
            with_markdown,
            built.pages.len(),
            format!("{with_markdown} page(s) have a `.md` route"),
        ),
    );
    push("content-negotiation", content_negotiation(built));

    // ---- Page Size ----
    let rendered = built
        .pages
        .iter()
        .filter(|page| page.server_rendered && !page.sparse)
        .count();
    push(
        "rendering-strategy",
        proportion(
            rendered,
            built.pages.len(),
            format!("{rendered} page(s) are fully server-rendered"),
        ),
    );
    push("page-size-markdown", page_size_markdown(built, options));
    push("page-size-html", page_size_html(built, options));
    push("page-size-transfer", page_size_transfer(built, options));
    push("content-start-position", content_start(built, options));
    push("single-fetch-completeness", single_fetch(built));

    // ---- Content Structure ----
    push("tabbed-content-serialization", tabbed(built, options));
    push("section-header-quality", header_quality(built));
    push("markdown-code-fence-validity", fence_validity(built));
    push("markdown-link-portability", link_portability(built));
    push("embedded-data-serialization", embedded_data(built));

    // ---- URL Stability ----
    push(
        "http-status-codes",
        verdict(
            built.headers.not_found_status == 404
                && built.surfaces.get(notfound::MARKDOWN_PATH).is_some(),
            "an unknown route returns 404 and the 404 body is published",
            format!(
                "an unknown route returns {}",
                built.headers.not_found_status
            ),
        ),
    );
    push("redirect-behavior", redirects(built));

    // ---- Observability ----
    push("llms-txt-coverage", coverage(built, index, options));
    push("markdown-content-parity", parity(built, options));
    push("cache-header-hygiene", cache_headers(built));

    // ---- Authentication ----
    let public = built.pages.iter().filter(|page| !page.gated).count();
    push(
        "auth-gate-detection",
        proportion(
            public,
            built.pages.len(),
            format!("{public} page(s) are public"),
        ),
    );
    push("auth-alternative-access", alternative_access(built));
    push(
        "bot-protection-interference",
        verdict(
            !built.headers.serves_challenge,
            "no challenge interstitial on a documentation route",
            "the host answers some documentation requests with a challenge",
        ),
    );

    out
}

// ---- individual checks ----

fn llms_txt_valid(index: Option<&str>) -> (Outcome, String) {
    let Some(body) = index else {
        return (Outcome::Fail, format!("no `{ROOT_PATH}`"));
    };
    let mut lines = body.lines();
    let heading = lines.next().is_some_and(|line| line.starts_with("# "));
    let summary = lines.any(|line| line.starts_with("> "));
    let sections = body.lines().any(|line| line.starts_with("## "));
    let entries = body.lines().any(|line| line.starts_with("- ["));
    let mut missing = Vec::new();
    for (ok, what) in [
        (heading, "an H1 title"),
        (summary, "a blockquote summary"),
        (sections, "heading-delimited sections"),
        (entries, "Markdown link entries"),
    ] {
        if !ok {
            missing.push(what);
        }
    }
    verdict(
        missing.is_empty(),
        "H1, summary, sections, and link entries",
        format!("missing {}", missing.join(", ")),
    )
}

/// The site-relative paths an index may point at: a page in either
/// representation, or a generated resource.
fn resolvable(built: &Built<'_>) -> BTreeSet<String> {
    let mut out: BTreeSet<String> = built
        .site
        .published()
        .flat_map(|page| {
            [
                built.site.origin.page_url(&page.route),
                built.site.origin.markdown_url(&page.route),
            ]
        })
        .collect();
    out.extend(
        built
            .surfaces
            .paths()
            .map(|path| built.site.origin.resource_url(path)),
    );
    out
}

fn internal_links(built: &Built<'_>, index: Option<&str>, options: &Options) -> Vec<String> {
    let Some(body) = index else { return Vec::new() };
    llms::link_targets(body)
        .into_iter()
        .map(|href| {
            if href.starts_with("http") {
                href
            } else {
                built.site.origin.resource_url(&href)
            }
        })
        .filter(|href| built.site.origin.contains(href))
        .take(options.max_links_to_test)
        .collect()
}

fn links_resolve(built: &Built<'_>, index: Option<&str>, options: &Options) -> (Outcome, String) {
    let known = resolvable(built);
    let links = internal_links(built, index, options);
    let dead: Vec<&String> = links.iter().filter(|href| !known.contains(*href)).collect();
    verdict(
        dead.is_empty(),
        format!("{} link(s) resolve", links.len()),
        format!(
            "{} link(s) resolve to nothing, starting with `{}`",
            dead.len(),
            dead.first().map_or("", |href| href.as_str())
        ),
    )
}

fn links_markdown(built: &Built<'_>, index: Option<&str>, options: &Options) -> (Outcome, String) {
    let links = internal_links(built, index, options);
    let pages: BTreeSet<String> = built
        .site
        .published()
        .map(|page| built.site.origin.page_url(&page.route))
        .collect();
    // An entry that points at a generated index rather than a page is the
    // split the spec asks for, not an HTML link (RX-70).
    let html_links: Vec<&String> = links.iter().filter(|href| pages.contains(*href)).collect();
    verdict(
        html_links.is_empty(),
        format!("{} link(s), all `.md` or generated indexes", links.len()),
        format!(
            "{} link(s) point at HTML, starting with `{}`",
            html_links.len(),
            html_links.first().map_or("", |href| href.as_str())
        ),
    )
}

fn directive_html(built: &Built<'_>) -> (Outcome, String) {
    let needle = built.site.origin.resource_url(ROOT_PATH);
    let carrying = built
        .pages
        .iter()
        .filter(|page| in_main(&page.html).contains(&needle))
        .count();
    proportion(
        carrying,
        built.pages.len(),
        format!("{carrying} page(s) carry the directive inside `main`"),
    )
}

/// What a check that ignores sidebar matches sees: the `main` element only.
fn in_main(html: &str) -> &str {
    let Some(open) = html.find("<main") else {
        return html;
    };
    let rest = &html[open..];
    match rest.find("</main>") {
        Some(close) => &rest[..close],
        None => rest,
    }
}

fn directive_md(built: &Built<'_>) -> (Outcome, String) {
    let expected = discovery_directive(&built.site.origin.resource_url(ROOT_PATH));
    let with_markdown: Vec<&BuiltPage> = built
        .pages
        .iter()
        .filter(|page| page.markdown.is_some())
        .collect();
    let carrying = with_markdown
        .iter()
        .filter(|page| {
            page.markdown
                .as_deref()
                .is_some_and(|body| body.starts_with(&expected))
        })
        .count();
    proportion(
        carrying,
        with_markdown.len(),
        format!("{carrying} Markdown route(s) open with the directive"),
    )
}

fn content_negotiation(built: &Built<'_>) -> (Outcome, String) {
    let headers = &built.headers;
    let typed = headers
        .markdown_content_type
        .as_deref()
        .is_some_and(|value| value.starts_with("text/markdown"));
    if headers.honours_accept && headers.vary_accept && typed {
        return (
            Outcome::Pass,
            "`Accept: text/markdown` answered with `Vary: Accept`".to_owned(),
        );
    }
    if typed {
        // A static host cannot negotiate; the Markdown route is the documented
        // partial answer (§25).
        return (
            Outcome::Warn,
            "a static host serves `.md` routes but cannot negotiate on the HTML route".to_owned(),
        );
    }
    (
        Outcome::Fail,
        "Markdown is not served as `text/markdown`".to_owned(),
    )
}

fn page_size_markdown(built: &Built<'_>, options: &Options) -> (Outcome, String) {
    let pages: Vec<&String> = built
        .pages
        .iter()
        .filter_map(|p| p.markdown.as_ref())
        .collect();
    let within = pages
        .iter()
        .filter(|body| body.chars().count() <= options.thresholds.markdown_error_chars)
        .count();
    proportion(
        within,
        pages.len(),
        format!(
            "{within} Markdown route(s) under {} characters",
            options.thresholds.markdown_error_chars
        ),
    )
}

/// The served HTML with `<script>` and `<style>` removed, which is what the
/// reference tool measures.
fn measured_html(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    loop {
        let next = ["<script", "<style"]
            .iter()
            .filter_map(|tag| rest.find(tag).map(|at| (at, *tag)))
            .min_by_key(|(at, _)| *at);
        let Some((at, tag)) = next else {
            out.push_str(rest);
            return out;
        };
        out.push_str(&rest[..at]);
        let close = if tag == "<script" {
            "</script>"
        } else {
            "</style>"
        };
        rest = match rest[at..].find(close) {
            Some(end) => &rest[at + end + close.len()..],
            None => return out,
        };
    }
}

fn page_size_html(built: &Built<'_>, options: &Options) -> (Outcome, String) {
    let within = built
        .pages
        .iter()
        .filter(|page| measured_html(&page.html).len() as u64 <= options.thresholds.html_bytes)
        .count();
    proportion(
        within,
        built.pages.len(),
        format!(
            "{within} page(s) under {} bytes of markup",
            options.thresholds.html_bytes
        ),
    )
}

fn page_size_transfer(built: &Built<'_>, options: &Options) -> (Outcome, String) {
    let within = built
        .pages
        .iter()
        .filter(|page| page.transfer_bytes <= options.thresholds.transfer_bytes)
        .count();
    proportion(
        within,
        built.pages.len(),
        format!(
            "{within} response(s) under {} bytes",
            options.thresholds.transfer_bytes
        ),
    )
}

fn content_start(built: &Built<'_>, options: &Options) -> (Outcome, String) {
    let early = built
        .pages
        .iter()
        .filter(|page| content_start_share(&page.html) <= options.thresholds.content_start)
        .count();
    proportion(
        early,
        built.pages.len(),
        format!(
            "{early} page(s) begin their content in the first {:.0}%",
            options.thresholds.content_start * 100.0
        ),
    )
}

/// Where the page's content starts as a share of the **converted** output, not
/// of the markup: `<head>` costs a reader nothing, a navigation tree ahead of
/// `main` costs them everything.
fn content_start_share(html: &str) -> f64 {
    let converted = strip_tags(&measured_html(html));
    if converted.is_empty() {
        return 1.0;
    }
    let before = match html.find("<main") {
        Some(at) => strip_tags(&measured_html(&html[..at])),
        // No `main` at all: the reader has to scan the whole document.
        None => return 1.0,
    };
    before.chars().count() as f64 / converted.chars().count() as f64
}

fn single_fetch(built: &Built<'_>) -> (Outcome, String) {
    let mut diagnostics = liyasa_core::diagnostics::Diagnostics::new();
    for page in built.pages {
        if let Some(body) = &page.markdown {
            continuation::check(page.route.as_str(), body, &mut diagnostics);
        }
    }
    for resource in &built.surfaces.resources {
        continuation::check(&resource.path, &resource.body, &mut diagnostics);
    }
    let split: Vec<&crate::agents::resource::Resource> = built
        .surfaces
        .resources
        .iter()
        .filter(|r| r.path.starts_with(llms::FULL_DIR) || r.path.starts_with(llms::INDEX_DIR))
        .collect();
    let declared = split
        .iter()
        .filter(|r| r.body.starts_with("> Part ") || !r.path.starts_with(llms::FULL_DIR))
        .count();
    verdict(
        diagnostics.is_empty() && declared == split.len(),
        format!(
            "no paginated route; {} split resource(s) declare their continuation first",
            split.len()
        ),
        "a resource declares its continuation as a trailing note".to_owned(),
    )
}

/// Tab groups serialize as `### Group: Tab` headings (RX-61).
fn tab_headings(markdown: &str) -> Vec<&str> {
    markdown
        .lines()
        .filter_map(|line| line.strip_prefix("### "))
        .filter(|heading| heading.contains(": "))
        .collect()
}

fn tabbed(built: &Built<'_>, options: &Options) -> (Outcome, String) {
    let tabbed: Vec<&BuiltPage> = built
        .pages
        .iter()
        .filter(|page| {
            page.markdown
                .as_deref()
                .is_some_and(|body| !tab_headings(body).is_empty())
        })
        .collect();
    if tabbed.is_empty() {
        return (
            Outcome::NotApplicable {
                reason: "no tabbed content on the sampled pages".to_owned(),
            },
            "no tabbed content".to_owned(),
        );
    }
    let bounded = tabbed
        .iter()
        .filter(|page| {
            page.markdown
                .as_deref()
                .is_some_and(|body| body.chars().count() <= options.thresholds.markdown_warn_chars)
        })
        .count();
    proportion(
        bounded,
        tabbed.len(),
        format!("{bounded} page(s) keep their tab groups within the size budget"),
    )
}

/// Titles that tell a reader nothing, which is what the check warns about.
const GENERIC_HEADERS: [&str; 6] = ["tab", "example", "option", "item", "section", "content"];

fn header_quality(built: &Built<'_>) -> (Outcome, String) {
    let mut total = 0;
    let mut specific = 0;
    for page in built.pages {
        let Some(body) = page.markdown.as_deref() else {
            continue;
        };
        for heading in tab_headings(body) {
            total += 1;
            let tab = heading.rsplit(": ").next().unwrap_or(heading);
            let bare = tab
                .trim()
                .trim_end_matches(|c: char| c.is_ascii_digit() || c == ' ')
                .to_ascii_lowercase();
            if !GENERIC_HEADERS.contains(&bare.as_str()) {
                specific += 1;
            }
        }
    }
    proportion(
        specific,
        total,
        format!("{specific} tab heading(s) name what they contain"),
    )
}

fn fence_validity(built: &Built<'_>) -> (Outcome, String) {
    let pages: Vec<&BuiltPage> = built
        .pages
        .iter()
        .filter(|page| page.markdown.is_some())
        .collect();
    let valid = pages
        .iter()
        .filter(|page| page.markdown.as_deref().is_some_and(fences_balanced))
        .count();
    proportion(
        valid,
        pages.len(),
        format!("{valid} Markdown route(s) close every fence"),
    )
}

fn fences_balanced(markdown: &str) -> bool {
    let mut open: Option<usize> = None;
    for line in markdown.lines() {
        let run = line.trim_start().chars().take_while(|c| *c == '`').count();
        match open {
            Some(len) if run >= len && line.trim_start().trim_end_matches('`').is_empty() => {
                open = None;
            }
            Some(_) => {}
            None if run >= 3 => open = Some(run),
            None => {}
        }
    }
    open.is_none()
}

fn link_portability(built: &Built<'_>) -> (Outcome, String) {
    let pages: Vec<&BuiltPage> = built
        .pages
        .iter()
        .filter(|page| page.markdown.is_some())
        .collect();
    let portable = pages
        .iter()
        .filter(|page| {
            page.markdown.as_deref().is_some_and(|body| {
                llms::link_targets(body)
                    .into_iter()
                    .all(|href| href.contains("://"))
            })
        })
        .count();
    proportion(
        portable,
        pages.len(),
        format!("{portable} Markdown route(s) link only absolute URLs"),
    )
}

fn embedded_data(built: &Built<'_>) -> (Outcome, String) {
    let measured: Vec<&size::Report> = built.pages.iter().filter_map(|p| p.size.as_ref()).collect();
    let clean = measured
        .iter()
        .filter(|report| !(report.is_over_warn() && report.bulk_share() >= size::BULK_DOMINANCE))
        .count();
    proportion(
        clean,
        measured.len(),
        format!("{clean} page(s) are not dominated by machine-generated bulk"),
    )
}

fn redirects(built: &Built<'_>) -> (Outcome, String) {
    if built.headers.javascript_redirects {
        return (
            Outcome::Fail,
            "the site redirects with markup or a script rather than a status".to_owned(),
        );
    }
    match built.headers.redirect_status {
        None => (
            Outcome::NotApplicable {
                reason: "the site declares no redirects".to_owned(),
            },
            "no redirects".to_owned(),
        ),
        Some(status @ (301 | 302 | 307 | 308)) => {
            (Outcome::Pass, format!("redirects answer with {status}"))
        }
        Some(status) => (Outcome::Fail, format!("redirects answer with {status}")),
    }
}

fn coverage(built: &Built<'_>, index: Option<&str>, options: &Options) -> (Outcome, String) {
    let Some(body) = index else {
        return (Outcome::Fail, format!("no `{ROOT_PATH}`"));
    };
    let counted: Vec<String> = built
        .site
        .published()
        .filter(|page| !options.excluded_from_coverage(page.route.as_str()))
        .map(|page| built.site.origin.markdown_url(&page.route))
        .collect();
    if counted.is_empty() {
        return (
            Outcome::NotApplicable {
                reason: "every page is excluded from coverage".to_owned(),
            },
            "nothing to cover".to_owned(),
        );
    }
    let mut linked: BTreeSet<String> = llms::link_targets(body).into_iter().collect();
    // A nested index is credited with the subtree it covers (RX-70).
    for resource in &built.surfaces.resources {
        if resource.path.starts_with(llms::INDEX_DIR) {
            linked.extend(llms::link_targets(&resource.body));
        }
    }
    let found = counted.iter().filter(|url| linked.contains(*url)).count();
    let share = found as f64 / counted.len() as f64;
    verdict(
        share >= options.thresholds.coverage,
        format!("{found} of {} indexable pages listed", counted.len()),
        format!(
            "{found} of {} indexable pages listed ({:.0}%)",
            counted.len(),
            share * 100.0
        ),
    )
}

/// Headings carry structure, and both representations come from one AST, so a
/// disagreement means something rewrote one of them.
fn parity(built: &Built<'_>, options: &Options) -> (Outcome, String) {
    let pages: Vec<&BuiltPage> = built
        .pages
        .iter()
        .filter(|page| {
            page.markdown.is_some() && !options.excluded_from_parity(page.route.as_str())
        })
        .collect();
    let matching = pages
        .iter()
        .filter(|page| {
            let markdown = page.markdown.as_deref().unwrap_or_default();
            markdown_headings(markdown) == html_headings(&page.html)
        })
        .count();
    proportion(
        matching,
        pages.len(),
        format!("{matching} page(s) have the same headings in both representations"),
    )
}

fn markdown_headings(markdown: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut in_fence = false;
    for line in markdown.lines() {
        let run = line.trim_start().chars().take_while(|c| *c == '`').count();
        if run >= 3 {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }
        if let Some(rest) = line.strip_prefix('#') {
            let text = rest.trim_start_matches('#').trim();
            if !text.is_empty() {
                out.push(text.to_owned());
            }
        }
    }
    out
}

fn html_headings(html: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = in_main(html);
    while let Some(at) = rest.find("<h") {
        let after = &rest[at + 2..];
        let Some(level) = after.chars().next().filter(char::is_ascii_digit) else {
            rest = after;
            continue;
        };
        let Some(open_end) = after.find('>') else {
            break;
        };
        let body = &after[open_end + 1..];
        let close = format!("</h{level}>");
        match body.find(&close) {
            Some(end) => {
                out.push(strip_tags(&body[..end]));
                rest = &body[end + close.len()..];
            }
            None => break,
        }
    }
    out
}

fn strip_tags(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut depth = 0u32;
    for ch in html.chars() {
        match ch {
            '<' => depth += 1,
            '>' => depth = depth.saturating_sub(1),
            _ if depth == 0 => out.push(ch),
            _ => {}
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn cache_headers(built: &Built<'_>) -> (Outcome, String) {
    let headers = &built.headers;
    let Some(cache_control) = headers.cache_control.as_deref() else {
        return (Outcome::Fail, "no `Cache-Control`".to_owned());
    };
    let fresh = cache_control.contains("max-age") && cache_control.contains("must-revalidate");
    if fresh && headers.etag && headers.last_modified {
        return (Outcome::Pass, format!("`{cache_control}` with validators"));
    }
    (
        Outcome::Warn,
        format!("`{cache_control}`; a validator or a directive is missing"),
    )
}

fn alternative_access(built: &Built<'_>) -> (Outcome, String) {
    let index_public = built
        .surfaces
        .get(ROOT_PATH)
        .is_some_and(crate::agents::resource::Resource::is_public);
    let public_markdown = built
        .pages
        .iter()
        .filter(|page| !page.gated && page.markdown.is_some())
        .count();
    verdict(
        index_public && public_markdown > 0,
        format!("`{ROOT_PATH}` is public and {public_markdown} Markdown route(s) are readable"),
        "a gated site with no public index and no public Markdown route".to_owned(),
    )
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::time::{Duration, UNIX_EPOCH};

    use liyasa_components::{nodes, registry::Registry};
    use liyasa_core::build::BuildClock;
    use liyasa_core::diagnostics::Diagnostics;
    use liyasa_core::document::{Block, BlockKind, Document, Node, Origin};
    use liyasa_core::ids::{BlockId, Locale};

    use super::super::score::Grade;
    use super::*;
    use crate::agents::markdown::{self, Options as MarkdownOptions};
    use crate::agents::site::{
        AgentsSettings, CanonicalOrigin, FeedsSettings, NavSection, PageRecord,
    };
    use crate::agents::{feeds, skill};

    const ORIGIN: &str = "https://example.com";

    struct Reference {
        site: SiteInput,
        surfaces: Surfaces,
        pages: Vec<BuiltPage>,
    }

    impl Reference {
        fn built(&self, headers: HostHeaders) -> Built<'_> {
            Built {
                site: &self.site,
                surfaces: &self.surfaces,
                pages: &self.pages,
                headers,
            }
        }
    }

    /// The pages of the reference site: enough of them to clear the
    /// insufficient-data rule, one with tabbed content, one changelog entry.
    fn page_specs() -> Vec<(&'static str, &'static str, Vec<&'static str>, bool)> {
        vec![
            ("/guide/install", "Install", vec!["Requirements"], false),
            (
                "/guide/configure",
                "Configure",
                vec!["Keys", "Defaults"],
                false,
            ),
            ("/guide/deploy", "Deploy", vec!["Static hosts"], false),
            ("/api/pets", "Pets", vec!["List pets"], false),
            ("/api/owners", "Owners", vec!["List owners"], false),
            ("/reference/cli", "CLI", vec!["Commands"], true),
            (
                "/changelog/2026-09",
                "September 2026",
                vec!["Changes"],
                false,
            ),
        ]
    }

    fn document(title: &str, sections: &[&str], tabbed: bool) -> Document {
        let mut children: Vec<Node> = vec![nodes::paragraph(&format!(
            "{title} in one paragraph, stated before anything else."
        ))];
        for section in sections {
            children.push(nodes::heading(2, section));
            children.push(nodes::paragraph(&format!(
                "Everything about {section}, in prose an agent can quote."
            )));
        }
        if tabbed {
            for (group, tab) in [("Install", "npm"), ("Install", "cargo")] {
                children.push(nodes::heading(3, &format!("{group}: {tab}")));
                children.push(nodes::code_block(
                    Some("sh"),
                    &format!("{tab} install liyasa"),
                ));
            }
        }
        Document {
            root: Block {
                id: BlockId::implicit("document", "", "", 0),
                explicit_id: None,
                kind: BlockKind::Document,
                origin: Origin::default(),
                children,
            },
            deps: Default::default(),
            diagnostics: Diagnostics::new(),
        }
    }

    /// The HTML the theme produces: `main` first in DOM order, the hidden
    /// discovery directive inside it, then the same headings the Markdown has.
    fn html(title: &str, sections: &[&str], tabbed: bool, llms_txt: &str) -> String {
        let mut out = String::from(
            "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"></head><body>",
        );
        out.push_str("<main>");
        out.push_str(&format!(
            "<p class=\"ly-visually-hidden\">For AI agents: a documentation index is available at {llms_txt}</p>"
        ));
        out.push_str(&format!("<h1>{title}</h1>"));
        out.push_str(&format!(
            "<p>{title} in one paragraph, stated before anything else.</p>"
        ));
        for section in sections {
            out.push_str(&format!("<h2>{section}</h2>"));
            out.push_str(&format!(
                "<p>Everything about {section}, in prose an agent can quote.</p>"
            ));
        }
        if tabbed {
            for tab in ["npm", "cargo"] {
                out.push_str(&format!("<h3>Install: {tab}</h3>"));
                out.push_str(&format!("<pre><code>{tab} install liyasa</code></pre>"));
            }
        }
        out.push_str("</main>");
        out.push_str(
            "<nav aria-label=\"Documentation\"><a href=\"/guide/install\">Install</a></nav>",
        );
        out.push_str("</body></html>");
        out
    }

    fn reference() -> Reference {
        let origin = CanonicalOrigin::parse(ORIGIN).expect("a valid origin");
        let registry = Registry::builtins();
        let routes: BTreeSet<Route> = page_specs()
            .iter()
            .map(|(route, ..)| Route::new(*route))
            .collect();

        let mut pages = Vec::new();
        let mut records = Vec::new();
        for (route, title, sections, tabbed) in page_specs() {
            let record = PageRecord {
                id: None,
                route: Route::new(route),
                title: title.to_owned(),
                description: Some(format!("What {title} is for, in one line.")),
                locale: Locale::new("en"),
                version: None,
                tab: Some(route.split('/').nth(1).unwrap_or("guide").to_owned()),
                group: None,
                indexable: true,
                personalized: false,
                markdown: String::new(),
                updated: Some("2026-09-15".to_owned()),
                changelog: route.starts_with("/changelog"),
            };
            let site_meta = liyasa_core::markdown::SiteMeta {
                name: "Liyasa".to_owned(),
                canonical_origin: origin.url().clone(),
                llms_txt: liyasa_core::net::Url::parse(&origin.resource_url(ROOT_PATH))
                    .expect("a valid URL"),
                version: None,
                locale: Locale::new("en"),
            };
            let frontmatter = liyasa_core::frontmatter::FrontmatterFields {
                title: Some(title.to_owned()),
                description: record.description.clone(),
                ..Default::default()
            };
            let doc = document(title, &sections, tabbed);
            let rendered = markdown::render_page(
                &doc,
                &MarkdownOptions {
                    site: &site_meta,
                    registry: &registry,
                    route: &record.route,
                    frontmatter: Some(&frontmatter),
                    routes: &routes,
                    site_instructions: None,
                    openapi_schema: None,
                },
            );
            assert!(
                rendered.diagnostics.is_empty(),
                "{:?}",
                rendered.diagnostics
            );

            let markup = html(title, &sections, tabbed, &origin.resource_url(ROOT_PATH));
            pages.push(BuiltPage {
                route: record.route.clone(),
                size: Some(size::Report {
                    chars: rendered.markdown.chars().count(),
                    bulk: Vec::new(),
                }),
                transfer_bytes: markup.len() as u64,
                markdown: Some(rendered.markdown.clone()),
                html: markup,
                gated: false,
                server_rendered: true,
                sparse: false,
            });
            records.push(PageRecord {
                markdown: rendered.markdown,
                ..record
            });
        }

        let site = SiteInput {
            name: "Liyasa".to_owned(),
            summary: Some("Documentation that agents and people can both read.".to_owned()),
            origin,
            locale: Locale::new("en"),
            version: None,
            nav: vec![
                NavSection {
                    title: "Guide".to_owned(),
                    tab: Some("guide".to_owned()),
                    routes: vec![
                        Route::new("/guide/install"),
                        Route::new("/guide/configure"),
                        Route::new("/guide/deploy"),
                    ],
                },
                NavSection {
                    title: "API".to_owned(),
                    tab: Some("api".to_owned()),
                    routes: vec![Route::new("/api/pets"), Route::new("/api/owners")],
                },
                NavSection {
                    title: "Reference".to_owned(),
                    tab: Some("reference".to_owned()),
                    routes: vec![Route::new("/reference/cli")],
                },
                NavSection {
                    title: "Changelog".to_owned(),
                    tab: Some("changelog".to_owned()),
                    routes: vec![Route::new("/changelog/2026-09")],
                },
            ],
            pages: records,
            agents: AgentsSettings::default(),
            feeds: FeedsSettings::default(),
        };

        let mut surfaces = llms::generate(&site);
        surfaces.absorb(skill::generate(&site));
        surfaces.absorb(notfound::generate(&site, None));
        surfaces.absorb(feeds::generate(
            &site,
            BuildClock(UNIX_EPOCH + Duration::from_secs(1_789_473_600)),
        ));
        assert!(
            !surfaces.diagnostics.has_errors(),
            "{:?}",
            surfaces.diagnostics
        );

        Reference {
            site,
            surfaces,
            pages,
        }
    }

    fn outcomes(report: &Report) -> Vec<(&str, String)> {
        report
            .results
            .iter()
            .map(|result| {
                let verdict = match &result.outcome {
                    Outcome::Pass => "pass",
                    Outcome::Warn | Outcome::Indeterminate => "warn",
                    Outcome::Fail => "fail",
                    Outcome::Partial { passed, total } if passed == total => "pass",
                    Outcome::Partial { .. } => "partial",
                    Outcome::Skipped { .. } => "skip",
                    Outcome::NotApplicable { .. } => "n/a",
                };
                let detail = match &result.outcome {
                    Outcome::Skipped { reason } | Outcome::NotApplicable { reason } => reason,
                    _ => &result.detail,
                };
                (result.id, format!("{verdict}: {detail}"))
            })
            .collect()
    }

    #[test]
    fn spec_01_the_reference_build_runs_all_twenty_eight_checks() {
        let reference = reference();
        let report = run(&reference.built(HostHeaders::server()), &Options::default());
        assert_eq!(report.results.len(), 28);
        assert_eq!(report.spec_version, "0.6.0");
    }

    #[test]
    fn spec_01_every_check_a_server_can_satisfy_passes_on_the_reference_build() {
        let reference = reference();
        let report = run(&reference.built(HostHeaders::server()), &Options::default());
        let failing: Vec<(&str, String)> = outcomes(&report)
            .into_iter()
            .filter(|(_, verdict)| verdict.starts_with("fail") || verdict.starts_with("partial"))
            .collect();
        assert!(failing.is_empty(), "{failing:#?}");
        assert!(report.findings.is_empty(), "{:#?}", report.findings);
    }

    #[test]
    fn spec_04_the_reference_build_scores_a_hundred_and_grades_a() {
        let reference = reference();
        let report = run(&reference.built(HostHeaders::server()), &Options::default());
        assert_eq!(report.score.comparable, 100, "{}", report.text());
        assert_eq!(report.score.grade, Grade::A);
        assert!(report.score.caps.is_empty(), "{:?}", report.score.caps);
        assert_eq!(report.score.discovery_coefficient, 1.0);
    }

    #[test]
    fn spec_01_a_static_host_is_partial_on_negotiation_and_nothing_else() {
        let reference = reference();
        let report = run(
            &reference.built(HostHeaders::default()),
            &Options::default(),
        );
        let soft: Vec<&str> = report
            .results
            .iter()
            .filter(|result| !result.outcome.is_pass() && result.outcome.counts())
            .map(|result| result.id)
            .collect();
        assert_eq!(soft, ["content-negotiation"]);
        // A static host still reaches an A: the Markdown routes carry it.
        assert_eq!(report.score.grade, Grade::A);
    }

    #[test]
    fn spec_01_a_missing_index_fails_discovery_and_skips_its_dependents() {
        let mut reference = reference();
        reference.surfaces.resources.retain(|r| r.path != ROOT_PATH);
        let report = run(&reference.built(HostHeaders::server()), &Options::default());
        assert!(
            report
                .result("llms-txt-exists")
                .expect("llms-txt-exists")
                .outcome
                .is_fail()
        );
        for id in ["llms-txt-valid", "llms-txt-size", "llms-txt-coverage"] {
            assert!(
                matches!(
                    report.result(id).expect(id).outcome,
                    Outcome::Skipped { .. }
                ),
                "{id}"
            );
        }
        assert_eq!(report.score.ceiling(), Some(59));
    }

    #[test]
    fn spec_01_a_site_with_no_markdown_routes_has_no_viable_path() {
        let mut reference = reference();
        for page in &mut reference.pages {
            page.markdown = None;
            page.server_rendered = false;
        }
        let report = run(
            &reference.built(HostHeaders::default()),
            &Options::default(),
        );
        assert!(report.facts.no_viable_path, "{}", report.text());
        assert!(
            report
                .findings
                .iter()
                .any(|f| f.effect == super::super::report::Effect::NoViableContentPath)
        );
        assert!(report.score.comparable <= 39, "{}", report.score.comparable);
    }

    #[test]
    fn spec_01_an_oversized_page_scores_the_check_proportionally() {
        let mut reference = reference();
        reference.pages[0].markdown = Some("x".repeat(120_000));
        let report = run(&reference.built(HostHeaders::server()), &Options::default());
        assert_eq!(
            report.result("page-size-markdown").expect("it").outcome,
            Outcome::Partial {
                passed: 6,
                total: 7
            }
        );
        assert!(report.score.comparable < 100);
        assert!(report.score.comparable > 95, "{}", report.score.comparable);
    }

    #[test]
    fn spec_01_a_relative_link_in_markdown_fails_portability() {
        let mut reference = reference();
        let body = reference.pages[0].markdown.take().unwrap_or_default();
        reference.pages[0].markdown = Some(format!("{body}\n\nSee [the guide](/guide/deploy).\n"));
        let report = run(&reference.built(HostHeaders::server()), &Options::default());
        assert_eq!(
            report
                .result("markdown-link-portability")
                .expect("it")
                .outcome,
            Outcome::Partial {
                passed: 6,
                total: 7
            }
        );
    }

    #[test]
    fn spec_01_an_unclosed_fence_fails_the_fence_check() {
        let mut reference = reference();
        let body = reference.pages[0].markdown.take().unwrap_or_default();
        reference.pages[0].markdown = Some(format!("{body}\n\n```sh\nliyasa build\n"));
        let report = run(&reference.built(HostHeaders::server()), &Options::default());
        assert!(
            !report
                .result("markdown-code-fence-validity")
                .expect("it")
                .outcome
                .is_pass()
        );
    }

    #[test]
    fn spec_01_tabbed_content_is_found_and_its_headers_are_judged() {
        let reference = reference();
        let report = run(&reference.built(HostHeaders::server()), &Options::default());
        assert_eq!(
            report
                .result("tabbed-content-serialization")
                .expect("it")
                .outcome,
            Outcome::Pass
        );
        assert_eq!(
            report.result("section-header-quality").expect("it").outcome,
            Outcome::Pass
        );
    }

    #[test]
    fn spec_01_a_generic_tab_title_fails_header_quality() {
        let mut reference = reference();
        let body = reference.pages[5]
            .markdown
            .take()
            .unwrap_or_default()
            .replace("### Install: npm", "### Install: Tab 1");
        reference.pages[5].markdown = Some(body);
        let report = run(&reference.built(HostHeaders::server()), &Options::default());
        assert!(
            !report
                .result("section-header-quality")
                .expect("it")
                .outcome
                .is_pass()
        );
    }

    #[test]
    fn spec_01_a_gated_site_caps_the_score_and_runs_the_alternative_check() {
        let mut reference = reference();
        for page in reference.pages.iter_mut().take(6) {
            page.gated = true;
        }
        let report = run(&reference.built(HostHeaders::server()), &Options::default());
        assert_eq!(report.score.ceiling(), Some(39));
        assert_eq!(
            report
                .result("auth-alternative-access")
                .expect("it")
                .outcome,
            Outcome::Pass
        );
    }

    #[test]
    fn spec_01_an_spa_shell_caps_the_score_and_flags_two_results() {
        let mut reference = reference();
        for page in &mut reference.pages {
            page.server_rendered = false;
        }
        let report = run(&reference.built(HostHeaders::server()), &Options::default());
        assert_eq!(report.score.ceiling(), Some(39));
        for id in ["page-size-html", "content-start-position"] {
            assert!(report.result(id).expect(id).unreliable, "{id}");
        }
    }

    #[test]
    fn spec_01_a_four_page_site_is_insufficient_data() {
        let mut reference = reference();
        reference.pages.truncate(4);
        reference.site.pages.truncate(4);
        let report = run(&reference.built(HostHeaders::server()), &Options::default());
        assert_eq!(report.score.ceiling(), Some(59));
        assert!(matches!(
            report.result("page-size-html").expect("it").outcome,
            Outcome::NotApplicable { .. }
        ));
    }

    #[test]
    fn spec_01_coverage_exclusions_are_honoured() {
        let mut reference = reference();
        reference.site.pages.push(PageRecord {
            id: None,
            route: Route::new("/internal/runbook"),
            title: "Runbook".to_owned(),
            description: None,
            locale: Locale::new("en"),
            version: None,
            tab: None,
            group: None,
            indexable: true,
            personalized: false,
            markdown: String::new(),
            updated: None,
            changelog: false,
        });
        // The index was generated before the page existed, so coverage drops.
        let strict = run(&reference.built(HostHeaders::server()), &Options::default());
        assert!(
            strict
                .result("llms-txt-coverage")
                .expect("it")
                .outcome
                .is_fail()
        );

        let options = Options {
            coverage_exclusions: vec!["/internal/*".to_owned()],
            ..Options::default()
        };
        let lenient = run(&reference.built(HostHeaders::server()), &options);
        assert_eq!(
            lenient.result("llms-txt-coverage").expect("it").outcome,
            Outcome::Pass
        );
    }

    #[test]
    fn spec_01_headings_that_disagree_fail_parity() {
        let mut reference = reference();
        reference.pages[0].html = reference.pages[0]
            .html
            .replace("<h2>Requirements</h2>", "<h2>Prerequisites</h2>");
        let report = run(&reference.built(HostHeaders::server()), &Options::default());
        assert_eq!(
            report
                .result("markdown-content-parity")
                .expect("it")
                .outcome,
            Outcome::Partial {
                passed: 6,
                total: 7
            }
        );

        let options = Options {
            parity_exclusions: vec!["/guide/install".to_owned()],
            ..Options::default()
        };
        assert_eq!(
            run(&reference.built(HostHeaders::server()), &options)
                .result("markdown-content-parity")
                .expect("it")
                .outcome,
            Outcome::Pass
        );
    }

    #[test]
    fn spec_01_a_challenge_interstitial_fails_bot_protection() {
        let reference = reference();
        let headers = HostHeaders {
            serves_challenge: true,
            ..HostHeaders::server()
        };
        let report = run(&reference.built(headers), &Options::default());
        assert!(
            report
                .result("bot-protection-interference")
                .expect("it")
                .outcome
                .is_fail()
        );
        assert!(
            report
                .findings
                .iter()
                .any(|f| f.effect == super::super::report::Effect::BotProtectionDegradingScan)
        );
    }

    #[test]
    fn spec_01_navigation_before_main_fails_content_start() {
        let nav = "<a>Install</a><a>Configure</a><a>Deploy</a><a>Pets</a><a>Owners</a>".repeat(20);
        let html = format!("<body><nav>{nav}</nav><main><h1>T</h1><p>Body.</p></main></body>");
        assert!(
            content_start_share(&html) > 0.10,
            "{}",
            content_start_share(&html)
        );

        let content_first =
            format!("<body><main><h1>T</h1><p>Body.</p></main><nav>{nav}</nav></body>");
        assert_eq!(content_start_share(&content_first), 0.0);
    }

    #[test]
    fn spec_01_a_page_without_main_fails_content_start() {
        assert_eq!(content_start_share("<body><p>Body.</p></body>"), 1.0);
    }

    #[test]
    fn spec_01_scripts_and_styles_are_stripped_before_html_is_measured() {
        let html =
            "<main><h1>T</h1><script>var x = 1;</script><style>a{}</style><p>Body.</p></main>";
        assert_eq!(measured_html(html), "<main><h1>T</h1><p>Body.</p></main>");
    }

    #[test]
    fn spec_01_only_main_is_searched_for_the_directive() {
        let html = "<body><nav>llms.txt</nav><main><p>body</p></main></body>";
        assert_eq!(in_main(html), "<main><p>body</p>");
    }

    #[test]
    fn spec_03_the_tracked_version_is_read_from_config() {
        let mut reference = reference();
        reference.site.agents.spec_version = "0.5.0".to_owned();
        let report = run(&reference.built(HostHeaders::server()), &Options::default());
        let codes: Vec<&str> = report
            .diagnostics(&Options::default())
            .iter()
            .map(|d| d.code.as_str())
            .collect();
        assert_eq!(codes, ["W0410"]);
    }
}
