//! CMP-102: everything the theme emits survives a strict CSP — no inline
//! handler, no inline script without the nonce, no `javascript:` URL.

use liyasa_theme::context::{Mode, RenderContext};
use liyasa_theme::nav::{Choice, Tab};
use liyasa_theme::theme::Theme;

fn pages() -> Vec<String> {
    let theme = Theme::new().expect("the theme builds");
    let mut out = Vec::new();

    let mut full = RenderContext::sample();
    full.nav.navigation.versions = vec![Choice {
        label: "v2".to_owned(),
        value: "v2".to_owned(),
        href: "/v2/".to_owned(),
        current: true,
    }];
    full.nav.navigation.locales = vec![Choice {
        label: "Deutsch".to_owned(),
        value: "de".to_owned(),
        href: "/de/".to_owned(),
        current: false,
    }];
    full.nav.navigation.tabs.push(Tab {
        title: "API".to_owned(),
        href: Some("/api".to_owned()),
        ..Tab::default()
    });
    out.push(theme.render_page(&full).expect("the page renders"));

    for mode in Mode::BUILT_IN {
        let mut context = RenderContext::sample();
        context.page.mode = Mode::parse(mode);
        out.push(
            theme
                .render_page(&context)
                .unwrap_or_else(|error| panic!("`{mode}` renders: {error}")),
        );
    }
    out
}

#[test]
fn no_element_the_theme_emits_carries_an_inline_handler() {
    for html in pages() {
        for (at, _) in html.match_indices(" on") {
            let rest = &html[at + 3..];
            let name: String = rest
                .chars()
                .take_while(|c| c.is_ascii_alphabetic())
                .collect();
            let followed_by_value = rest[name.len()..].starts_with('=');
            assert!(
                !followed_by_value,
                "`on{name}=` is an inline handler, which a nonce-based CSP blocks"
            );
        }
    }
}

#[test]
fn every_inline_script_and_style_carries_the_nonce() {
    for html in pages() {
        for (at, _) in html.match_indices("<script") {
            let tag = &html[at..html[at..].find('>').map_or(html.len(), |end| at + end)];
            let inline = !tag.contains(" src=");
            let data = tag.contains("type=\"application/json\"");
            if inline && !data {
                assert!(tag.contains("nonce=\""), "`{tag}` has no nonce");
            }
        }
        for (at, _) in html.match_indices("<style") {
            let tag = &html[at..html[at..].find('>').map_or(html.len(), |end| at + end)];
            assert!(tag.contains("nonce=\""), "`{tag}` has no nonce");
        }
    }
}

#[test]
fn nothing_the_theme_emits_navigates_through_javascript() {
    for html in pages() {
        assert!(!html.contains("javascript:"));
    }
}
