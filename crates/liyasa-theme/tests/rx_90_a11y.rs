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
    let styles = Styles::build(&ThemeConfig::default(), &Tokens::aurora(), &[]);
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

    let styles = Styles::build(&ThemeConfig::default(), &Tokens::aurora(), &[]);
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
