//! CMP-100: every theme element carries a stable `data-liyasa` attribute, and
//! the documented list and the rendered markup are the same set.

use std::collections::BTreeSet;

use liyasa_theme::config::ThemeConfig;
use liyasa_theme::context::{Mode, RenderContext};
use liyasa_theme::nav::{Breadcrumbs, Choice, Link, Tab};
use liyasa_theme::stylesheet::Styles;
use liyasa_theme::theme::{ELEMENTS, Theme};
use liyasa_theme::tokens::Tokens;

fn attributes(html: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut rest = html;
    while let Some(at) = rest.find("data-liyasa=\"") {
        rest = &rest[at + "data-liyasa=\"".len()..];
        if let Some(end) = rest.find('"') {
            out.insert(rest[..end].to_owned());
            rest = &rest[end..];
        }
    }
    out
}

fn rendered() -> BTreeSet<String> {
    let theme = Theme::new().expect("the theme builds");
    let mut found = BTreeSet::new();

    let context = RenderContext::sample();
    found.extend(attributes(
        &theme.render_page(&context).expect("the page renders"),
    ));

    // The rail's panel replaces the table of contents, so one render cannot
    // show both (RX-23).
    let mut with_panel = RenderContext::sample();
    with_panel.set_panel("<p>rail</p>");
    found.extend(attributes(
        &theme.render_page(&with_panel).expect("the panel renders"),
    ));

    // The assistant layout adds its own trigger.
    let mut assistant = RenderContext::sample();
    assistant.page.mode = Mode::Assistant;
    found.extend(attributes(
        &theme.render_page(&assistant).expect("the mode renders"),
    ));

    // A site with tabs, switchers, an eyebrow, and neighbours renders the
    // elements the sample page has no occasion to.
    let mut full = RenderContext::sample();
    full.nav.breadcrumbs = Breadcrumbs::Eyebrow;
    full.nav.navigation.tabs.push(Tab {
        title: "API".to_owned(),
        href: Some("/api".to_owned()),
        ..Tab::default()
    });
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
    full.page.previous = Some(Link {
        title: "Introduction".to_owned(),
        route: "/introduction".to_owned(),
    });
    full.page.next = Some(Link {
        title: "Configure".to_owned(),
        route: "/configure".to_owned(),
    });
    found.extend(attributes(
        &theme.render_page(&full).expect("the full page renders"),
    ));

    for partial in ["code-block", "callout", "component"] {
        found.extend(attributes(
            &theme
                .render_partial(partial, &context)
                .unwrap_or_else(|error| panic!("`{partial}` renders: {error}")),
        ));
    }
    found
}

#[test]
fn the_documented_elements_and_the_rendered_ones_are_the_same_set() {
    let found = rendered();
    let documented: BTreeSet<String> = ELEMENTS.iter().map(|name| (*name).to_owned()).collect();

    let missing: Vec<&String> = documented.difference(&found).collect();
    assert!(
        missing.is_empty(),
        "documented but never rendered: {missing:?}"
    );

    let extra: Vec<&String> = found.difference(&documented).collect();
    assert!(extra.is_empty(), "rendered but not documented: {extra:?}");
}

#[test]
fn the_documented_list_is_sorted_and_free_of_duplicates() {
    let mut sorted = ELEMENTS.to_vec();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(sorted, ELEMENTS, "the element list is sorted and unique");
}

#[test]
fn a_selector_on_a_documented_element_matches_what_the_theme_styles() {
    // The attribute is the stable hook; the class is documented alongside it,
    // so a stylesheet written against either keeps working.
    let styles = Styles::build(&ThemeConfig::default(), &Tokens::aurora(), &[]);
    for (element, class) in [
        ("sidebar", ".ly-sidebar"),
        ("navbar", ".ly-navbar"),
        ("toc", ".ly-toc-list"),
        ("footer", ".ly-footer"),
        ("callout", ".ly-callout"),
        ("code-block", ".ly-code"),
    ] {
        assert!(ELEMENTS.contains(&element), "`{element}` is documented");
        assert!(
            styles.css.contains(class),
            "`{class}` is documented for `{element}` but the theme does not style it"
        );
    }
}

#[test]
fn a_custom_stylesheet_can_target_the_attribute() {
    let styles = Styles::build(
        &ThemeConfig::default(),
        &Tokens::aurora(),
        &["[data-liyasa=\"sidebar\"] { border-right: 0; }"],
    );
    assert!(
        styles
            .css
            .contains("[data-liyasa=\"sidebar\"]{border-right:0}")
    );
}
