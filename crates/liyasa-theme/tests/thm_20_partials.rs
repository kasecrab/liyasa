//! THM-20 and THM-21: every named partial and every layout can be replaced by
//! a file of the same name, and the override is what renders.

use std::collections::BTreeMap;

use liyasa_theme::config::ThemeConfig;
use liyasa_theme::context::{Mode, RenderContext, partial_names};
use liyasa_theme::theme::{Overrides, Theme};

fn rendered_with(partial: &str, source: &str) -> String {
    let overrides = Overrides {
        partials: BTreeMap::from([(partial.to_owned(), source.to_owned())]),
        ..Overrides::default()
    };
    let theme = Theme::with_overrides(&ThemeConfig::default(), &overrides)
        .unwrap_or_else(|error| panic!("`{partial}` override loads: {error}"));
    theme
        .render_page(&RenderContext::sample())
        .unwrap_or_else(|error| panic!("`{partial}` override renders: {error}"))
}

#[test]
fn every_partial_thm_20_names_is_overridable() {
    for partial in partial_names() {
        // `component`, `code-block`, and `callout` are rendered by a component,
        // not by the page layout; their override path is the same registry.
        if matches!(partial, "component" | "code-block" | "callout") {
            let overrides = Overrides {
                partials: BTreeMap::from([(
                    partial.to_owned(),
                    format!("<!-- override:{partial} -->"),
                )]),
                ..Overrides::default()
            };
            let theme = Theme::with_overrides(&ThemeConfig::default(), &overrides)
                .expect("the override loads");
            let html = theme
                .render_partial(partial, &RenderContext::sample())
                .expect("the partial renders");
            assert_eq!(html.trim(), format!("<!-- override:{partial} -->"));
            continue;
        }
        let marker = format!("marker-for-{partial}");
        let html = rendered_with(partial, &format!("<div>{marker}</div>"));
        assert!(
            html.contains(&marker),
            "`{partial}` was overridden but the default still rendered"
        );
    }
}

#[test]
fn the_default_partial_is_used_when_there_is_no_override() {
    let theme = Theme::new(&ThemeConfig::default()).expect("the theme builds");
    let html = theme
        .render_page(&RenderContext::sample())
        .expect("the page renders");
    assert!(html.contains("data-liyasa=\"footer\""));
    assert!(!theme.is_overridden("footer"));
}

#[test]
fn every_shipped_partial_is_documented_and_the_reverse() {
    let (undocumented, unshipped) = liyasa_theme::theme::undocumented();
    assert!(
        undocumented.is_empty(),
        "shipped but undocumented: {undocumented:?}"
    );
    assert!(
        unshipped.is_empty(),
        "documented but not shipped: {unshipped:?}"
    );
}

#[test]
fn an_operator_can_add_a_mode() {
    let overrides = Overrides {
        layouts: BTreeMap::from([(
            "gallery".to_owned(),
            "{% extends \"layouts/base\" %}{% block main %}<p>gallery</p>{% endblock %}".to_owned(),
        )]),
        ..Overrides::default()
    };
    let theme =
        Theme::with_overrides(&ThemeConfig::default(), &overrides).expect("the layout loads");
    let mut context = RenderContext::sample();
    context.page.mode = Mode::parse("gallery");
    let html = theme.render_page(&context).expect("the new mode renders");
    assert!(html.contains("<p>gallery</p>"));
    assert!(theme.modes().contains(&"gallery".to_owned()));
}

#[test]
fn an_operator_can_replace_a_built_in_layout() {
    let overrides = Overrides {
        layouts: BTreeMap::from([("wide".to_owned(), "<p>ours</p>".to_owned())]),
        ..Overrides::default()
    };
    let theme =
        Theme::with_overrides(&ThemeConfig::default(), &overrides).expect("the layout loads");
    let mut context = RenderContext::sample();
    context.page.mode = Mode::Wide;
    assert_eq!(
        theme.render_page(&context).expect("the override renders"),
        "<p>ours</p>"
    );
}

#[test]
fn a_mode_with_no_layout_is_a_diagnostic_not_a_panic() {
    let theme = Theme::new(&ThemeConfig::default()).expect("the theme builds");
    let mut context = RenderContext::sample();
    context.page.mode = Mode::parse("nonexistent");
    let error = theme
        .render_page(&context)
        .expect_err("there is no such mode");
    let diagnostic = error.diagnostic();
    assert_eq!(diagnostic.code.as_str(), "E0202");
    assert!(diagnostic.message.contains("nonexistent"));
}

#[test]
fn a_broken_override_is_a_diagnostic_naming_the_partial() {
    let overrides = Overrides {
        partials: BTreeMap::from([("footer".to_owned(), "{% for %}".to_owned())]),
        ..Overrides::default()
    };
    let error = Theme::with_overrides(&ThemeConfig::default(), &overrides)
        .expect_err("a syntax error is reported");
    let diagnostic = error.diagnostic();
    assert_eq!(diagnostic.code.as_str(), "E0202");
    assert!(
        diagnostic.message.contains("footer"),
        "{}",
        diagnostic.message
    );
}
