//! `llms.txt`, its sub-indexes, and `llms-full.txt` (RX-70, RX-71, RX-72).
//!
//! The root index is the door an agent finds first, so it is the one file that
//! is always small enough to read whole: when the site outgrows the 50,000
//! characters the spec's `llms-txt-size` check allows, the sections move into
//! `/_llms/` indexes and the root becomes the list of them.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use liyasa_core::diagnostics::{Diagnostic, Diagnostics, code};
use liyasa_core::ids::Route;

use crate::agents::continuation::Continuation;
use crate::agents::markdown::discovery_directive;
use crate::agents::site::{PageRecord, SiteInput};

/// The ceiling the spec's `llms-txt-size` check applies to the root index.
pub const INDEX_MAX_CHARS: usize = 50_000;

pub const ROOT_PATH: &str = "/llms.txt";
pub const FULL_PATH: &str = "/llms-full.txt";
pub const INDEX_DIR: &str = "/_llms";
pub const FULL_DIR: &str = "/_llms/full";

/// The section a sub-index split is declared in, first in the root file so the
/// declaration survives truncation (RX-64).
const SPLIT_HEADING: &str = "Indexes";

/// Where pages that no navigation section lists are collected, so coverage is
/// 100% of indexable pages rather than 100% of navigated ones.
const OTHER_SECTION: &str = "Other";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resource {
    /// Site-absolute path, such as `/llms.txt`.
    pub path: String,
    pub body: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Surfaces {
    pub resources: Vec<Resource>,
    pub diagnostics: Diagnostics,
}

impl Surfaces {
    pub fn get(&self, path: &str) -> Option<&Resource> {
        self.resources.iter().find(|r| r.path == path)
    }
}

/// Generates the root index, any sub-indexes it needs, and `llms-full.txt`.
pub fn generate(site: &SiteInput) -> Surfaces {
    let mut out = Surfaces::default();
    let sections = sections_of(site);

    match site.agents.llms.custom.as_deref() {
        Some(custom) => custom_index(site, custom, &mut out),
        None => generated_index(site, &sections, &mut out),
    }
    if site.agents.llms.full {
        full_text(site, &sections, &mut out);
    }
    for resource in &out.resources {
        crate::agents::continuation::check(&resource.path, &resource.body, &mut out.diagnostics);
    }
    out
}

// ---- the root index (RX-70) ----

/// Every section the index lists, navigation first, then whatever navigation
/// left out.
fn sections_of(site: &SiteInput) -> Vec<Section<'_>> {
    let mut listed: BTreeSet<&Route> = BTreeSet::new();
    let mut sections: Vec<Section<'_>> = Vec::new();
    for nav in &site.nav {
        let pages: Vec<&PageRecord> = nav
            .routes
            .iter()
            .filter_map(|route| site.page(route))
            .filter(|page| page.is_published())
            .collect();
        if pages.is_empty() {
            continue;
        }
        listed.extend(pages.iter().map(|page| &page.route));
        sections.push(Section {
            title: nav.title.clone(),
            tab: nav.tab.clone(),
            pages,
        });
    }
    let rest: Vec<&PageRecord> = site
        .published()
        .filter(|page| !listed.contains(&page.route))
        .collect();
    if !rest.is_empty() {
        sections.push(Section {
            title: OTHER_SECTION.to_owned(),
            tab: None,
            pages: rest,
        });
    }
    sections
}

struct Section<'a> {
    title: String,
    tab: Option<String>,
    pages: Vec<&'a PageRecord>,
}

fn generated_index(site: &SiteInput, sections: &[Section<'_>], out: &mut Surfaces) {
    let whole = index_body(site, &header(site), sections);
    if whole.chars().count() <= INDEX_MAX_CHARS || !site.agents.llms.split {
        out.resources.push(Resource {
            path: ROOT_PATH.to_owned(),
            body: whole,
        });
        return;
    }
    split_index(site, sections, out);
}

fn header(site: &SiteInput) -> String {
    let summary = site.summary.as_deref().unwrap_or(&site.name);
    format!("# {}\n\n> {summary}\n", site.name)
}

fn index_body(site: &SiteInput, header: &str, sections: &[Section<'_>]) -> String {
    let mut out = header.to_owned();
    for section in sections {
        let _ = write!(out, "\n## {}\n\n", section.title);
        for page in &section.pages {
            out.push_str(&entry(site, page));
        }
    }
    out
}

/// One `llms.txt` line: an absolute `.md` link and a one-line description.
fn entry(site: &SiteInput, page: &PageRecord) -> String {
    let url = site.origin.markdown_url(&page.route);
    match page
        .description
        .as_deref()
        .map(str::trim)
        .filter(|d| !d.is_empty())
    {
        Some(description) => format!("- [{}]({url}): {}\n", page.title, one_line(description)),
        None => format!("- [{}]({url})\n", page.title),
    }
}

fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Moves every section into a `/_llms/` index and leaves the root as the list
/// of them, so the root is small whatever the site's size (RX-70).
fn split_index(site: &SiteInput, sections: &[Section<'_>], out: &mut Surfaces) {
    let mut by_tab: BTreeMap<String, Vec<&Section<'_>>> = BTreeMap::new();
    for section in sections {
        let tab = section
            .tab
            .clone()
            .unwrap_or_else(|| OTHER_SECTION.to_owned());
        by_tab.entry(tab).or_default().push(section);
    }

    let mut root = header(site);
    // RX-64: the split is declared in the opening section, with absolute URLs,
    // so a pipeline that truncates the root still knows the rest exists.
    let _ = write!(root, "\n## {SPLIT_HEADING}\n\n");
    let mut indexes = Vec::new();
    for (tab, sections) in &by_tab {
        let path = format!("{INDEX_DIR}/{}.txt", slug(tab));
        let url = site.origin.resource_url(&path);
        let pages: usize = sections.iter().map(|s| s.pages.len()).sum();
        let _ = writeln!(
            root,
            "- [{tab}]({url}): {pages} page{} in {} section{}",
            plural(pages),
            sections.len(),
            plural(sections.len()),
        );
        let header = format!("# {} — {tab}\n\n> {} pages.\n", site.name, pages);
        let owned: Vec<Section<'_>> = sections
            .iter()
            .map(|s| Section {
                title: s.title.clone(),
                tab: s.tab.clone(),
                pages: s.pages.clone(),
            })
            .collect();
        indexes.push(Resource {
            path,
            body: index_body(site, &header, &owned),
        });
    }
    out.resources.push(Resource {
        path: ROOT_PATH.to_owned(),
        body: root,
    });
    out.resources.extend(indexes);
}

fn plural(count: usize) -> &'static str {
    if count == 1 { "" } else { "s" }
}

fn slug(text: &str) -> String {
    let slug = liyasa_components::anchor::slug(text);
    if slug.is_empty() {
        "index".to_owned()
    } else {
        slug
    }
}

// ---- a repository's own llms.txt (RX-72) ----

/// Publishes a custom index verbatim and checks what it points at: an override
/// is allowed to be wrong about the site, but not silently.
fn custom_index(site: &SiteInput, body: &str, out: &mut Surfaces) {
    let published: BTreeSet<String> = site
        .published()
        .map(|page| site.origin.markdown_url(&page.route))
        .collect();
    let routes: BTreeSet<String> = site
        .published()
        .flat_map(|page| {
            [
                site.origin.markdown_url(&page.route),
                site.origin.page_url(&page.route),
            ]
        })
        .collect();

    let mut linked = BTreeSet::new();
    for href in link_targets(body) {
        if href.starts_with("http") && !site.origin.contains(&href) {
            // An off-site link is the author's business, not a broken route.
            continue;
        }
        let absolute = if href.starts_with("http") {
            href.clone()
        } else {
            site.origin.resource_url(&href)
        };
        if routes.contains(&absolute) {
            linked.insert(absolute.trim_end_matches(".md").to_owned());
            linked.insert(absolute);
            continue;
        }
        out.diagnostics.push(
            Diagnostic::new(
                code::W0408,
                format!("`{}` links to `{href}`, which is not a route this build publishes", site.agents.llms.custom.as_deref().unwrap_or(ROOT_PATH)),
            )
            .help("remove the link or add the page; a listed route that 404s costs the whole index its `llms-txt-links-resolve` score"),
        );
    }

    let missing: Vec<&String> = published
        .iter()
        .filter(|url| !linked.contains(*url))
        .collect();
    if !missing.is_empty() {
        let shown: Vec<&str> = missing.iter().take(5).map(|url| url.as_str()).collect();
        out.diagnostics.push(
            Diagnostic::new(
                code::W0409,
                format!(
                    "{} of {} indexable page{} are missing from the custom index: {}{}",
                    missing.len(),
                    published.len(),
                    plural(published.len()),
                    shown.join(", "),
                    if missing.len() > shown.len() {
                        ", …"
                    } else {
                        ""
                    },
                ),
            )
            .help("`llms-txt-coverage` scores the share of indexable pages the index lists"),
        );
    }

    out.resources.push(Resource {
        path: ROOT_PATH.to_owned(),
        body: body.to_owned(),
    });
}

/// Every Markdown link destination in a body, fenced code skipped.
pub fn link_targets(body: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut fence: Option<String> = None;
    for line in body.lines() {
        let trimmed = line.trim_start();
        let run = trimmed.chars().take_while(|c| *c == '`').count();
        match &fence {
            Some(open) if run >= open.len() => {
                fence = None;
                continue;
            }
            Some(_) => continue,
            None if run >= 3 => {
                fence = Some("`".repeat(run));
                continue;
            }
            None => {}
        }
        let mut rest = line;
        while let Some(at) = rest.find("](") {
            rest = &rest[at + 2..];
            let end = rest.find(')').unwrap_or(rest.len());
            let href = rest[..end].split_whitespace().next().unwrap_or_default();
            if !href.is_empty() {
                out.push(href.to_owned());
            }
            rest = &rest[end.min(rest.len())..];
        }
    }
    out
}

// ---- llms-full.txt (RX-71) ----

fn full_text(site: &SiteInput, sections: &[Section<'_>], out: &mut Surfaces) {
    let pages = ordered_pages(sections);
    let cap = site.agents.llms.full_max_bytes.max(1) as usize;
    let whole = concatenate(site, &pages);
    if whole.len() <= cap || !site.agents.llms.split {
        out.resources.push(Resource {
            path: FULL_PATH.to_owned(),
            body: whole,
        });
        return;
    }
    split_full_text(site, &pages, cap, out);
}

/// Navigation order, each page once.
fn ordered_pages<'a>(sections: &[Section<'a>]) -> Vec<&'a PageRecord> {
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for section in sections {
        for page in &section.pages {
            if seen.insert(&page.route) {
                out.push(*page);
            }
        }
    }
    out
}

fn concatenate(site: &SiteInput, pages: &[&PageRecord]) -> String {
    let mut out = String::new();
    for page in pages {
        out.push_str(&full_entry(site, page));
    }
    out
}

/// One page inside the full text: its H1, its source URL, then its body.
fn full_entry(site: &SiteInput, page: &PageRecord) -> String {
    let body = strip_preamble(&page.markdown);
    format!(
        "# {}\n\nSource: {}\n\n{}\n\n",
        page.title,
        site.origin.markdown_url(&page.route),
        body.trim()
    )
}

/// Drops the per-page discovery blockquote and H1: the full text carries its
/// own separators, and repeating the directive once per page would make the
/// file mostly directive.
fn strip_preamble(markdown: &str) -> &str {
    let mut rest = markdown;
    if rest.starts_with("> For AI agents:") {
        rest = rest.split_once("\n\n").map_or("", |(_, tail)| tail);
    }
    if rest.starts_with("# ") {
        rest = rest.split_once('\n').map_or("", |(_, tail)| tail);
    }
    rest.trim_start_matches('\n')
}

/// Packs pages into parts keyed by tab or group, each under the cap, and
/// turns the root into an ordered index of them (RX-71).
fn split_full_text(site: &SiteInput, pages: &[&PageRecord], cap: usize, out: &mut Surfaces) {
    let mut parts: Vec<(String, String)> = Vec::new();
    let mut key_counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut current: Option<(String, String)> = None;

    for page in pages {
        let key = page
            .tab
            .clone()
            .or_else(|| page.group.clone())
            .unwrap_or_else(|| OTHER_SECTION.to_owned());
        let entry = full_entry(site, page);
        let same_key = current.as_ref().is_some_and(|(k, _)| *k == key);
        let fits = current
            .as_ref()
            .is_some_and(|(_, body)| body.len() + entry.len() <= cap);
        if !same_key || !fits {
            if let Some(part) = current.take() {
                parts.push(part);
            }
            current = Some((key.clone(), String::new()));
        }
        if let Some((_, body)) = current.as_mut() {
            body.push_str(&entry);
        }
    }
    if let Some(part) = current.take() {
        parts.push(part);
    }

    let paths: Vec<String> = parts
        .iter()
        .map(|(key, _)| {
            let count = key_counts.entry(key.clone()).or_default();
            *count += 1;
            match *count {
                1 => format!("{FULL_DIR}/{}.txt", slug(key)),
                n => format!("{FULL_DIR}/{}-{n}.txt", slug(key)),
            }
        })
        .collect();
    let urls: Vec<String> = paths
        .iter()
        .map(|path| site.origin.resource_url(path))
        .collect();
    let of = site.origin.resource_url(FULL_PATH);

    let mut root = format!(
        "# {} — full text\n\n> The full text of this site is published in {} parts.\n\n",
        site.name,
        parts.len()
    );
    for (at, (key, body)) in parts.iter().enumerate() {
        let continuation = Continuation {
            part: at + 1,
            total: parts.len(),
            of: of.clone(),
            previous: (at > 0).then(|| urls[at - 1].clone()),
            next: urls.get(at + 1).cloned(),
        };
        let part_body = continuation.open(body);
        let _ = writeln!(
            root,
            "- [Part {} — {key}]({}): {}",
            at + 1,
            urls[at],
            human_bytes(part_body.len())
        );
        out.resources.push(Resource {
            path: paths[at].clone(),
            body: part_body,
        });
    }
    out.resources.insert(
        0,
        Resource {
            path: FULL_PATH.to_owned(),
            body: root,
        },
    );
}

fn human_bytes(bytes: usize) -> String {
    const UNITS: [&str; 4] = ["bytes", "KB", "MB", "GB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} bytes")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

/// The share of indexable pages an index lists, which is what
/// `llms-txt-coverage` scores.
pub fn coverage(site: &SiteInput, index: &str) -> f64 {
    let published: Vec<String> = site
        .published()
        .map(|page| site.origin.markdown_url(&page.route))
        .collect();
    if published.is_empty() {
        return 1.0;
    }
    let linked: BTreeSet<String> = link_targets(index).into_iter().collect();
    let found = published.iter().filter(|url| linked.contains(*url)).count();
    found as f64 / published.len() as f64
}

/// The `llms.txt` directive line an HTML page carries, kept here so the root
/// index and the page directive cannot disagree about the URL (RX-01, CM-141).
pub fn directive_for(site: &SiteInput) -> String {
    discovery_directive(&site.origin.resource_url(ROOT_PATH))
}

#[cfg(test)]
mod tests {
    use liyasa_core::ids::Locale;

    use super::*;
    use crate::agents::site::{AgentsSettings, CanonicalOrigin, FeedsSettings, NavSection};

    fn page(route: &str, title: &str, tab: Option<&str>) -> PageRecord {
        PageRecord {
            id: None,
            route: Route::new(route),
            title: title.to_owned(),
            description: Some(format!("What {title} is for.")),
            locale: Locale::new("en"),
            version: None,
            tab: tab.map(str::to_owned),
            group: None,
            indexable: true,
            personalized: false,
            markdown: format!(
                "> For AI agents: a documentation index is available at \
                 https://example.com/llms.txt\n\n# {title}\n\nWhat {title} is for.\n\nBody of {title}.\n"
            ),
            updated: None,
            changelog: false,
        }
    }

    fn site(pages: Vec<PageRecord>, nav: Vec<NavSection>) -> SiteInput {
        SiteInput {
            name: "Liyasa".to_owned(),
            summary: Some("Documentation that agents and people can both read.".to_owned()),
            origin: CanonicalOrigin::parse("https://example.com").expect("a valid origin"),
            locale: Locale::new("en"),
            version: None,
            pages,
            nav,
            agents: AgentsSettings::default(),
            feeds: FeedsSettings::default(),
        }
    }

    fn small_site() -> SiteInput {
        site(
            vec![
                page("/guide/install", "Install", Some("Guide")),
                page("/guide/config", "Configure", Some("Guide")),
                page("/api/pets", "Pets", Some("API")),
            ],
            vec![
                NavSection {
                    title: "Getting started".to_owned(),
                    tab: Some("Guide".to_owned()),
                    routes: vec![Route::new("/guide/install"), Route::new("/guide/config")],
                },
                NavSection {
                    title: "Endpoints".to_owned(),
                    tab: Some("API".to_owned()),
                    routes: vec![Route::new("/api/pets")],
                },
            ],
        )
    }

    fn root_of(surfaces: &Surfaces) -> &str {
        &surfaces.get(ROOT_PATH).expect("a root index").body
    }

    #[test]
    fn rx_70_the_index_parses_as_llmstxt_asks() {
        let surfaces = generate(&small_site());
        let root = root_of(&surfaces);
        let mut lines = root.lines();
        assert_eq!(lines.next(), Some("# Liyasa"));
        assert_eq!(lines.next(), Some(""));
        assert_eq!(
            lines.next(),
            Some("> Documentation that agents and people can both read.")
        );
        assert!(root.contains("\n## Getting started\n"), "{root}");
        assert!(root.contains("\n## Endpoints\n"), "{root}");
    }

    #[test]
    fn rx_70_every_link_is_an_absolute_md_route() {
        let surfaces = generate(&small_site());
        let targets = link_targets(root_of(&surfaces));
        assert!(!targets.is_empty());
        for target in targets {
            assert!(target.starts_with("https://example.com/"), "{target}");
            assert!(target.ends_with(".md"), "{target}");
        }
    }

    #[test]
    fn rx_70_every_entry_carries_a_one_line_description() {
        let surfaces = generate(&small_site());
        assert!(
            root_of(&surfaces).contains(
                "- [Install](https://example.com/guide/install.md): What Install is for."
            ),
            "{}",
            root_of(&surfaces)
        );
    }

    #[test]
    fn rx_70_the_index_covers_every_indexable_page() {
        let site = small_site();
        let surfaces = generate(&site);
        assert_eq!(coverage(&site, root_of(&surfaces)), 1.0);
    }

    #[test]
    fn rx_70_a_page_no_section_lists_is_still_covered() {
        let mut site = small_site();
        site.pages.push(page("/changelog", "Changelog", None));
        let surfaces = generate(&site);
        let root = root_of(&surfaces);
        assert!(root.contains("\n## Other\n"), "{root}");
        assert_eq!(coverage(&site, root), 1.0);
    }

    #[test]
    fn rx_70_a_personalized_page_never_reaches_the_index() {
        let mut site = small_site();
        let mut private = page("/dashboard", "Dashboard", None);
        private.personalized = true;
        site.pages.push(private);
        let surfaces = generate(&site);
        assert!(
            !root_of(&surfaces).contains("Dashboard"),
            "{}",
            root_of(&surfaces)
        );
    }

    #[test]
    fn rx_70_a_noindex_page_never_reaches_the_index() {
        let mut site = small_site();
        let mut hidden = page("/internal", "Internal", None);
        hidden.indexable = false;
        site.pages.push(hidden);
        assert!(!root_of(&generate(&site)).contains("Internal"));
    }

    /// Enough pages that the root index cannot hold them all.
    fn large_site() -> SiteInput {
        let tabs = ["Guide", "API", "Reference"];
        let mut pages = Vec::new();
        let mut nav = Vec::new();
        for tab in tabs {
            let mut routes = Vec::new();
            for n in 0..400 {
                let route = format!("/{}/page-{n:04}", tab.to_ascii_lowercase());
                pages.push(page(
                    &route,
                    &format!("{tab} page {n:04} with a title long enough to matter"),
                    Some(tab),
                ));
                routes.push(Route::new(&route));
            }
            nav.push(NavSection {
                title: format!("{tab} section"),
                tab: Some(tab.to_owned()),
                routes,
            });
        }
        site(pages, nav)
    }

    #[test]
    fn rx_70_an_oversized_index_splits_and_the_root_stays_small() {
        let surfaces = generate(&large_site());
        let root = root_of(&surfaces);
        assert!(
            root.chars().count() <= INDEX_MAX_CHARS,
            "{}",
            root.chars().count()
        );
        assert!(root.contains("## Indexes"), "{root}");
        for tab in ["guide", "api", "reference"] {
            let path = format!("{INDEX_DIR}/{tab}.txt");
            let index = surfaces.get(&path).unwrap_or_else(|| panic!("{path}"));
            assert!(index.body.contains("- ["), "{}", index.body);
        }
    }

    #[test]
    fn rx_70_the_split_is_declared_in_the_root_with_absolute_urls() {
        let surfaces = generate(&large_site());
        let root = root_of(&surfaces);
        let heading = root.find("## Indexes").expect("the split section");
        let first_entry = root.find("- [").expect("an entry");
        assert!(heading < first_entry, "{root}");
        for target in link_targets(root) {
            assert!(target.starts_with("https://example.com/_llms/"), "{target}");
        }
    }

    #[test]
    fn rx_71_the_full_text_carries_every_page_with_a_source_url() {
        let surfaces = generate(&small_site());
        let full = &surfaces.get(FULL_PATH).expect("the full text").body;
        assert!(
            full.contains("# Install\n\nSource: https://example.com/guide/install.md"),
            "{full}"
        );
        assert!(full.contains("Body of Install."), "{full}");
        assert!(
            full.contains("# Pets\n\nSource: https://example.com/api/pets.md"),
            "{full}"
        );
        assert!(!full.contains("For AI agents:"), "{full}");
    }

    #[test]
    fn rx_71_a_full_text_over_the_cap_becomes_an_index_of_parts() {
        let mut site = large_site();
        site.agents.llms.full_max_bytes = 64 * 1024;
        let surfaces = generate(&site);
        let root = &surfaces.get(FULL_PATH).expect("the full text").body;
        assert!(root.contains("published in"), "{root}");
        let parts: Vec<&Resource> = surfaces
            .resources
            .iter()
            .filter(|r| r.path.starts_with(FULL_DIR))
            .collect();
        assert!(parts.len() > 1, "{}", parts.len());
        for part in &parts {
            assert!(part.body.len() <= 64 * 1024 + 512, "{}", part.body.len());
        }
    }

    #[test]
    fn rx_71_every_part_opens_with_its_continuation_header() {
        let mut site = large_site();
        site.agents.llms.full_max_bytes = 64 * 1024;
        let surfaces = generate(&site);
        let parts: Vec<&Resource> = surfaces
            .resources
            .iter()
            .filter(|r| r.path.starts_with(FULL_DIR))
            .collect();
        let total = parts.len();
        for (at, part) in parts.iter().enumerate() {
            let mut lines = part.body.lines();
            assert_eq!(
                lines.next(),
                Some(
                    format!(
                        "> Part {} of {total} of https://example.com/llms-full.txt",
                        at + 1
                    )
                    .as_str()
                ),
                "{}",
                part.path
            );
            assert!(lines.next().expect("previous").starts_with("> Previous: "));
            assert!(lines.next().expect("next").starts_with("> Next: "));
        }
    }

    #[test]
    fn rx_71_the_root_index_lists_the_parts_in_order_with_sizes() {
        let mut site = large_site();
        site.agents.llms.full_max_bytes = 64 * 1024;
        let surfaces = generate(&site);
        let root = &surfaces.get(FULL_PATH).expect("the full text").body;
        let numbers: Vec<usize> = root
            .lines()
            .filter_map(|line| line.strip_prefix("- [Part "))
            .filter_map(|rest| rest.split_whitespace().next())
            .filter_map(|n| n.parse().ok())
            .collect();
        assert!(!numbers.is_empty());
        assert!(numbers.windows(2).all(|w| w[1] == w[0] + 1), "{numbers:?}");
        assert!(root.contains(" KB") || root.contains(" MB"), "{root}");
    }

    #[test]
    fn rx_71_no_split_resource_trails_its_continuation() {
        let mut site = large_site();
        site.agents.llms.full_max_bytes = 64 * 1024;
        let surfaces = generate(&site);
        assert!(
            !surfaces.diagnostics.has_errors(),
            "{:?}",
            surfaces.diagnostics
        );
    }

    #[test]
    fn rx_72_a_custom_index_is_published_verbatim() {
        let mut site = small_site();
        let custom = "# Liyasa\n\n> Hand written.\n\n## Start\n\n- [Install](/guide/install.md)\n- [Configure](/guide/config.md)\n- [Pets](/api/pets.md)\n";
        site.agents.llms.custom = Some(custom.to_owned());
        let surfaces = generate(&site);
        assert_eq!(root_of(&surfaces), custom);
        assert!(
            surfaces.diagnostics.is_empty(),
            "{:?}",
            surfaces.diagnostics
        );
    }

    #[test]
    fn rx_72_a_custom_index_with_a_dead_link_warns() {
        let mut site = small_site();
        site.agents.llms.custom = Some(
            "# Liyasa\n\n> Hand written.\n\n## Start\n\n- [Install](/guide/install.md)\n- [Configure](/guide/config.md)\n- [Pets](/api/pets.md)\n- [Gone](/guide/removed.md)\n"
                .to_owned(),
        );
        let surfaces = generate(&site);
        let codes: Vec<&str> = surfaces
            .diagnostics
            .iter()
            .map(|d| d.code.as_str())
            .collect();
        assert_eq!(codes, ["W0408"]);
    }

    #[test]
    fn rx_72_a_custom_index_that_misses_pages_warns_about_coverage() {
        let mut site = small_site();
        site.agents.llms.custom = Some(
            "# Liyasa\n\n> Hand written.\n\n## Start\n\n- [Install](/guide/install.md)\n"
                .to_owned(),
        );
        let surfaces = generate(&site);
        let coverage = surfaces
            .diagnostics
            .iter()
            .find(|d| d.code.as_str() == "W0409")
            .expect("W0409");
        assert!(coverage.message.contains("2 of 3"), "{}", coverage.message);
    }

    #[test]
    fn rx_72_an_external_link_in_a_custom_index_is_not_a_dead_route() {
        let mut site = small_site();
        site.agents.llms.custom = Some(
            "# Liyasa\n\n> Hand written.\n\n## Start\n\n- [Install](/guide/install.md)\n- [Configure](/guide/config.md)\n- [Pets](/api/pets.md)\n- [Spec](https://agentdocsspec.com/)\n"
                .to_owned(),
        );
        assert!(generate(&site).diagnostics.is_empty());
    }

    #[test]
    fn a_link_inside_a_fence_is_not_a_link() {
        let body =
            "# T\n\n```md\n- [Not a link](/nowhere.md)\n```\n\n- [Real](/guide/install.md)\n";
        assert_eq!(link_targets(body), ["/guide/install.md"]);
    }
}
