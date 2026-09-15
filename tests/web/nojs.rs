//! THM-31 without a browser: every page is readable and navigable with
//! JavaScript disabled.
//!
//! `web/reader/e2e/nojs.spec.ts` is the requirement's acceptance test and it
//! drives a real browser, which this machine cannot install (RFC 1100,
//! NEEDS-INPUT). Everything that is a property of the served HTML rather than
//! of a running page is asserted here instead, so the guarantee is covered by
//! a suite that actually runs: the enhancement may not be load-bearing.

use std::collections::HashSet;

use liyasa_tests::site::{self, Page};

/// Shorter than this and the page is chrome with a heading on it.
const MIN_TEXT: usize = 200;

fn pages() -> Vec<Page> {
    site::build().expect("the reference site renders").pages
}

/// The opening tag containing `needle`, attributes and all.
fn tag_with<'a>(html: &'a str, needle: &str) -> Option<&'a str> {
    let at = html.find(needle)?;
    let start = html[..at].rfind('<')?;
    let end = html[at..].find('>')? + at;
    Some(&html[start..=end])
}

fn region<'a>(html: &'a str, open: &str, close: &str) -> Option<&'a str> {
    let start = html.find(open)?;
    let end = html[start..].find(close)? + start;
    Some(&html[start..end])
}

/// The text a reader sees, with tags and script bodies removed.
fn text(html: &str) -> String {
    let mut out = String::new();
    let mut depth = 0usize;
    for character in html.chars() {
        match character {
            '<' => depth += 1,
            '>' => depth = depth.saturating_sub(1),
            _ if depth == 0 => out.push(character),
            _ => {}
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn ids(html: &str) -> HashSet<&str> {
    let mut found = HashSet::new();
    let mut rest = html;
    while let Some(at) = rest.find("id=\"") {
        rest = &rest[at + 4..];
        if let Some(end) = rest.find('"') {
            found.insert(&rest[..end]);
            rest = &rest[end..];
        }
    }
    found
}

/// Every `href="#…"` in `html`, without the `#`.
fn fragments(html: &str) -> Vec<&str> {
    let mut found = Vec::new();
    let mut rest = html;
    while let Some(at) = rest.find("href=\"#") {
        rest = &rest[at + 7..];
        if let Some(end) = rest.find('"') {
            found.push(&rest[..end]);
            rest = &rest[end..];
        }
    }
    found
}

#[test]
fn every_page_carries_its_content_in_the_html() {
    for page in pages() {
        let main = region(&page.html, "<main", "</main>")
            .unwrap_or_else(|| panic!("`{}` has no main element", page.route));
        let length = text(main).len();
        assert!(
            length >= MIN_TEXT,
            "`{}` renders {length} characters of text without a script",
            page.route
        );
    }
}

#[test]
fn nothing_in_the_page_waits_for_a_script_to_be_revealed() {
    for page in pages() {
        let main = region(&page.html, "<main", "</main>")
            .unwrap_or_else(|| panic!("`{}` has no main element", page.route));
        // A `hidden` element inside the page is content a reader without the
        // runtime never sees. Chrome may hide a control it also enhances; the
        // page itself may not.
        assert!(
            !main.contains(" hidden"),
            "`{}` hides part of the page until a script runs",
            page.route
        );
    }
}

#[test]
fn the_skip_link_reaches_the_page() {
    for page in pages() {
        let link = tag_with(&page.html, "data-liyasa=\"skip-link\"")
            .unwrap_or_else(|| panic!("`{}` has no skip link", page.route));
        assert!(
            link.starts_with("<a ") && link.contains("href=\"#ly-main\""),
            "`{}`'s skip link is not an anchor to the page: {link}",
            page.route
        );
        assert!(
            ids(&page.html).contains("ly-main"),
            "`{}` has no `ly-main` to skip to",
            page.route
        );
    }
}

#[test]
fn the_sidebar_opens_without_a_script() {
    for page in pages() {
        let trigger = tag_with(&page.html, "data-ly-drawer-trigger")
            .unwrap_or_else(|| panic!("`{}` has no drawer trigger", page.route));
        assert!(
            trigger.starts_with("<a ") && trigger.contains("href=\"#ly-sidebar\""),
            "`{}`'s drawer trigger is a dead button without a script: {trigger}",
            page.route
        );
        assert!(
            ids(&page.html).contains("ly-sidebar"),
            "`{}` has no `ly-sidebar` for the trigger to open",
            page.route
        );
    }
}

#[test]
fn search_offers_a_page_rather_than_a_dead_button() {
    for page in pages() {
        let trigger = tag_with(&page.html, "data-ly-search-trigger")
            .unwrap_or_else(|| panic!("`{}` has no search trigger", page.route));
        assert!(
            trigger.contains(" hidden"),
            "`{}`'s search button is offered before its module can answer it",
            page.route
        );
        let fallback = region(&page.html, "<noscript>", "</noscript>")
            .unwrap_or_else(|| panic!("`{}` offers no search without a script", page.route));
        assert!(
            fallback.contains("href=\"/search\""),
            "`{}`'s noscript fallback does not link to the search page",
            page.route
        );
    }
}

#[test]
fn the_table_of_contents_links_into_the_page() {
    for page in pages() {
        let toc = region(&page.html, "data-liyasa=\"toc\"", "</nav>")
            .unwrap_or_else(|| panic!("`{}` has no table of contents", page.route));
        let entries = fragments(toc);
        if entries.is_empty() {
            continue; // A page with no headings has nothing to list.
        }
        let ids = ids(&page.html);
        for entry in entries {
            assert!(
                ids.contains(entry),
                "`{}`'s table of contents points at `#{entry}`, which is not on the page",
                page.route
            );
        }
    }
}

#[test]
fn the_markdown_twin_is_linked_from_the_head() {
    for page in pages() {
        let count = page.html.matches("type=\"text/markdown\"").count();
        assert_eq!(
            count, 1,
            "`{}` links {count} Markdown twins; a reader and an agent need exactly one",
            page.route
        );
    }
}

#[test]
fn the_document_does_not_claim_a_script_has_run() {
    for page in pages() {
        let head = region(&page.html, "<html", ">")
            .unwrap_or_else(|| panic!("`{}` has no html element", page.route));
        assert!(
            !head.contains("data-ly-js"),
            "`{}` is served already marked as scripted: {head}",
            page.route
        );
    }
}

#[test]
fn navigation_between_pages_is_anchors() {
    for page in pages() {
        let sidebar = region(&page.html, "data-liyasa=\"sidebar-nav\"", "</nav>")
            .unwrap_or_else(|| panic!("`{}` has no sidebar navigation", page.route));
        let links = sidebar.matches("<a ").count();
        assert!(
            links > 0,
            "`{}`'s sidebar offers no anchor a reader without a script can follow",
            page.route
        );
    }
}

/// Every class on an element the markup renders `hidden`.
fn hidden_classes(html: &str) -> Vec<&str> {
    let mut found = Vec::new();
    let mut rest = html;
    while let Some(at) = rest.find('<') {
        rest = &rest[at..];
        let Some(end) = rest.find('>') else { break };
        let tag = &rest[..=end];
        rest = &rest[end..];
        if !tag.contains(" hidden") {
            continue;
        }
        let Some(class_at) = tag.find("class=\"") else {
            continue;
        };
        let names = &tag[class_at + 7..];
        let Some(close) = names.find('"') else {
            continue;
        };
        found.extend(names[..close].split_whitespace());
    }
    found
}

/// Every `selector { declarations }` pair in `css`, `@media` wrappers skipped.
fn rules(css: &str) -> Vec<(&str, &str)> {
    let bytes = css.as_bytes();
    let mut out = Vec::new();
    let mut selector_from = 0;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'{' => {
                let mut depth = 1;
                let mut j = i + 1;
                while j < bytes.len() && depth > 0 {
                    match bytes[j] {
                        b'{' => depth += 1,
                        b'}' => depth -= 1,
                        _ => {}
                    }
                    j += 1;
                }
                let body = &css[i + 1..j.saturating_sub(1)];
                let selector = css[selector_from..i].trim();
                if body.contains('{') {
                    // An at-rule wrapper: its own rules are found by walking in.
                    i += 1;
                    selector_from = i;
                    continue;
                }
                if !selector.starts_with('@') {
                    out.push((selector, body));
                }
                i = j;
                selector_from = i;
            }
            b'}' => {
                i += 1;
                selector_from = i;
            }
            _ => i += 1,
        }
    }
    out
}

/// Does any selector in the list name exactly `.class`, with no suffix?
fn names_class(selector: &str, class: &str) -> bool {
    selector.split(',').any(|one| {
        one.split_whitespace().any(|part| {
            part.strip_prefix('.')
                .and_then(|rest| rest.strip_prefix(class))
                .is_some_and(|tail| tail.is_empty())
        })
    })
}

#[test]
fn a_control_the_markup_hides_is_not_displayed_anyway() {
    let site = site::build().expect("the reference site renders");
    let mut classes: Vec<&str> = Vec::new();
    for page in &site.pages {
        classes.extend(hidden_classes(&page.html));
    }
    classes.sort_unstable();
    classes.dedup();

    let rules = rules(&site.stylesheet);
    let mut overriding = Vec::new();
    for class in classes {
        let sets_display = rules
            .iter()
            .any(|(selector, body)| names_class(selector, class) && body.contains("display:"));
        if !sets_display {
            continue; // `[hidden]` from the browser's own stylesheet holds.
        }
        let neutralised = rules.iter().any(|(selector, body)| {
            selector.contains(&format!(".{class}[hidden]")) && body.contains("display:")
        });
        if !neutralised {
            overriding.push(class);
        }
    }

    assert!(
        overriding.is_empty(),
        "these classes set `display` and never take it back under `[hidden]`, \
         so the control is a dead button without a script: {overriding:?}"
    );
}
