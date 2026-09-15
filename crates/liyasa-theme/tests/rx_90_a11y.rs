//! RX-90: landmarks, skip link, focus, ARIA, reduced motion, and the
//! announcement channel for route changes.

use liyasa_theme::config::ThemeConfig;
use liyasa_theme::context::RenderContext;
use liyasa_theme::stylesheet::Styles;
use liyasa_theme::theme::Theme;
use liyasa_theme::tokens::Tokens;

fn page() -> String {
    Theme::new()
        .expect("the theme builds")
        .render_page(&RenderContext::sample())
        .expect("the page renders")
}

#[test]
fn the_page_has_one_of_each_landmark() {
    let html = page();
    // The assistant panel's own `<header>` sits inside a `<section>`, so it is
    // not a second banner landmark; the page-level ones are counted here.
    for (landmark, count) in [
        ("<header class=\"ly-navbar\"", 1),
        ("<main", 1),
        ("<footer class=\"ly-footer\"", 1),
    ] {
        assert_eq!(
            html.matches(landmark).count(),
            count,
            "`{landmark}` appears the wrong number of times"
        );
    }
    assert!(html.contains("<aside class=\"ly-sidebar\""));
    assert!(html.contains("<aside class=\"ly-rail\""));
    assert!(
        html.contains("<nav "),
        "navigation regions are nav elements"
    );
}

#[test]
fn the_skip_link_is_the_first_focusable_thing_and_it_targets_main() {
    let html = page();
    let skip = html.find("ly-skip-link").expect("the skip link renders");
    for focusable in ["<a class=\"ly-navbar-logo\"", "<button"] {
        let at = html.find(focusable).expect("the element renders");
        assert!(skip < at, "`{focusable}` comes before the skip link");
    }
    assert!(html.contains("href=\"#ly-main\""));
    assert!(html.contains("id=\"ly-main\" tabindex=\"-1\""));
}

#[test]
fn every_navigation_region_is_labelled() {
    let html = page();
    for region in [
        "data-liyasa=\"breadcrumbs\"",
        "data-liyasa=\"toc\"",
        "data-liyasa=\"sidebar-nav\"",
    ] {
        let at = html.find(region).expect("the region renders");
        let window = &html[at.saturating_sub(200)..(at + 200).min(html.len())];
        assert!(
            window.contains("aria-label") || window.contains("aria-labelledby"),
            "`{region}` has no accessible name"
        );
    }
}

#[test]
fn state_is_exposed_to_assistive_technology() {
    let html = page();
    assert!(
        html.contains("aria-current=\"page\""),
        "the active page is marked"
    );
    assert!(
        html.contains("aria-expanded="),
        "disclosure state is exposed"
    );
    assert!(html.contains("aria-haspopup=\"dialog\""));
    assert!(html.contains("role=\"status\" aria-live=\"polite\""));
    assert!(
        html.contains("aria-modal=\"true\""),
        "the search overlay is a dialog"
    );
    assert!(
        !html.contains("tabindex=\"1\""),
        "a positive tabindex reorders the document for keyboard users"
    );
}

#[test]
fn every_image_the_theme_emits_carries_alt_text() {
    let html = page();
    for (at, _) in html.match_indices("<img ") {
        let tag = &html[at..html[at..].find('>').map_or(html.len(), |end| at + end)];
        assert!(tag.contains("alt=\""), "`{tag}` has no alt attribute");
    }
}

#[test]
fn the_stylesheet_carries_focus_and_reduced_motion() {
    let styles =
        Styles::build(&ThemeConfig::default(), &Tokens::aurora(), &[]).expect("the theme compiles");
    assert!(styles.css.contains(":focus-visible"));
    assert!(
        styles
            .css
            .contains("outline:var(--ly-border-width-strong) solid var(--ly-color-focus)")
    );
    assert!(
        styles
            .css
            .contains("@media (prefers-reduced-motion:reduce)")
    );
    assert!(
        styles.css.contains(".ly-visually-hidden"),
        "the theme has a class for text only screen readers see"
    );
}

#[test]
fn no_transition_outlasts_two_hundred_milliseconds() {
    // THM-05: micro-interactions stay under 200 ms. The durations are tokens,
    // so checking the tokens checks every transition that uses them.
    let tokens = Tokens::aurora();
    for name in [
        "--ly-motion-instant",
        "--ly-motion-fast",
        "--ly-motion-base",
        "--ly-motion-slow",
    ] {
        let value = tokens
            .get(name, liyasa_theme::tokens::Scheme::Light)
            .unwrap_or_else(|| panic!("`{name}` has a value"));
        let milliseconds: u32 = value
            .trim_end_matches("ms")
            .parse()
            .unwrap_or_else(|_| panic!("`{name}` is `{value}`, which is not a duration in ms"));
        assert!(milliseconds <= 200, "`{name}` is {milliseconds}ms");
    }

    let styles =
        Styles::build(&ThemeConfig::default(), &Tokens::aurora(), &[]).expect("the theme compiles");
    for (at, _) in styles.css.match_indices("transition:") {
        let rule = &styles.css[at..(at + 200).min(styles.css.len())];
        let literal = rule
            .split(['}', ';'])
            .next()
            .unwrap_or_default()
            .split_whitespace()
            .find(|part| {
                part.ends_with("ms") && part.chars().next().is_some_and(|c| c.is_ascii_digit())
            });
        if let Some(literal) = literal {
            let milliseconds: u32 = literal.trim_end_matches("ms").parse().unwrap_or(0);
            assert!(milliseconds <= 200, "a literal {milliseconds}ms transition");
        }
    }
}

/// Elements that carry no closing tag, so they never open a nesting level.
const VOID: &[&str] = &["area", "br", "col", "hr", "img", "input", "source", "wbr"];

/// One element: its opening tag, its tag name, and the text it contains.
struct Element<'a> {
    open: &'a str,
    name: &'a str,
    text: String,
}

fn tag_name(tag: &str) -> &str {
    tag.trim_start_matches(['<', '/'])
        .split([' ', '>', '/', '\n'])
        .next()
        .unwrap_or_default()
}

fn classes(tag: &str) -> Vec<&str> {
    tag.find("class=\"")
        .map(|at| &tag[at + 7..])
        .and_then(|rest| rest.find('"').map(|end| &rest[..end]))
        .map(|names| names.split_whitespace().collect())
        .unwrap_or_default()
}

/// The text `inner` carries directly, and one entry per child element with the
/// text that child carries. A name assembled from children is only as reliable
/// as the children are visible.
fn parts(inner: &str) -> (String, Vec<Element<'_>>) {
    let mut direct = String::new();
    let mut children: Vec<Element> = Vec::new();
    let mut open: Vec<(&str, usize)> = Vec::new();
    let mut rest = inner;
    while let Some(at) = rest.find('<') {
        let text = &rest[..at];
        match open.first() {
            None => direct.push_str(text),
            Some(_) => children.last_mut().expect("a child is open").text += text,
        }
        rest = &rest[at..];
        let Some(end) = rest.find('>') else { break };
        let tag = &rest[..=end];
        let name = tag_name(tag);
        if tag.starts_with("</") {
            open.pop();
        } else if !tag.ends_with("/>") && !VOID.contains(&name) {
            if open.is_empty() {
                children.push(Element {
                    open: tag,
                    name,
                    text: String::new(),
                });
            }
            open.push((name, children.len()));
        }
        rest = &rest[end + 1..];
    }
    if open.is_empty() {
        direct.push_str(rest);
    }
    (direct.trim().to_owned(), children)
}

/// Every `<button>` in `html`, as (opening tag, inner HTML).
fn buttons(html: &str) -> Vec<(&str, &str)> {
    let mut found = Vec::new();
    let mut rest = html;
    while let Some(at) = rest.find("<button") {
        rest = &rest[at..];
        let Some(open_end) = rest.find('>') else {
            break;
        };
        let Some(close) = rest.find("</button>") else {
            break;
        };
        found.push((&rest[..=open_end], &rest[open_end + 1..close]));
        rest = &rest[close..];
    }
    found
}

/// Every selector in `css` whose rule sets `display: none`, split into its
/// whitespace-separated compounds. Selectors carrying an attribute or a
/// pseudo-class are skipped: those hide an element in a state, not always.
fn hiding_selectors(css: &str) -> Vec<Vec<&str>> {
    let mut found = Vec::new();
    let mut rest = css;
    while let Some(at) = rest.find('{') {
        let selector = rest[..at].trim();
        rest = &rest[at + 1..];
        let Some(end) = rest.find('}') else { break };
        let body = &rest[..end];
        if body.contains("display:none") && !selector.starts_with('@') {
            for one in selector.split(',') {
                let one = one.trim();
                if one.contains('[') || one.contains(':') {
                    continue;
                }
                found.push(one.split_whitespace().collect());
            }
        }
        rest = &rest[end + 1..];
    }
    found
}

fn matches(compound: &str, element_name: &str, element_classes: &[&str]) -> bool {
    match compound.strip_prefix('.') {
        Some(class) => element_classes.contains(&class),
        None => compound == element_name,
    }
}

/// Can the stylesheet hide `child` when it sits inside `button`?
fn hideable(selectors: &[Vec<&str>], button: &str, child: &Element<'_>) -> bool {
    let button_classes = classes(button);
    let child_classes = classes(child.open);
    selectors.iter().any(|compounds| {
        let Some((last, ancestors)) = compounds.split_last() else {
            return false;
        };
        matches(last, child.name, &child_classes)
            && ancestors.iter().all(|ancestor| {
                matches(ancestor, "button", &button_classes)
                    || matches(ancestor, child.name, &child_classes)
            })
    })
}

#[test]
fn every_button_has_a_name_the_stylesheet_cannot_take_away() {
    // A button labelled only by a child the stylesheet hides is unnamed at the
    // widths where that rule applies, and an icon is `aria-hidden`, so the name
    // has to come from the button itself or from text nothing can hide.
    let html = page();
    let styles =
        Styles::build(&ThemeConfig::default(), &Tokens::aurora(), &[]).expect("the theme compiles");
    let selectors = hiding_selectors(&styles.css);

    let mut unnamed = Vec::new();
    for (open, inner) in buttons(&html) {
        if open.contains("aria-label=\"") || open.contains("aria-labelledby=\"") {
            continue;
        }
        let (direct, children) = parts(inner);
        let named = !direct.is_empty()
            || children.iter().any(|child| {
                !child.text.trim().is_empty()
                    && !child.open.contains("aria-hidden=\"true\"")
                    && !hideable(&selectors, open, child)
            });
        if !named {
            unnamed.push(open.to_owned());
        }
    }

    assert!(
        unnamed.is_empty(),
        "these buttons have no accessible name at every width: {unnamed:#?}"
    );
}
