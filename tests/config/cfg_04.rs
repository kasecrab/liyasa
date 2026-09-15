//! CFG-04: the colour roles, the contrast check, and the tokens they become.

use liyasa_core::diagnostics::Severity;
use liyasa_tests::config::{codes, one, project};
use liyasa_theme::color::Color;
use liyasa_theme::config::ThemeConfig;
use liyasa_theme::tokens::{Scheme, Tokens};

const PAGES: &[(&str, &str)] = &[("index.md", "# Home")];

fn with_config(config: &str) -> Vec<(&str, &str)> {
    let mut files = vec![("liyasa.json", config)];
    files.extend_from_slice(PAGES);
    files
}

/// Every role CFG-04 lists, with values that pass the contrast check.
const COLORS: &str = r##"{
  "name": "Acme",
  "seo": { "canonicalOrigin": "https://acme.dev" },
  "theme": { "colors": {
    "primary": "#4338CA",
    "light": "#4338CA",
    "dark": "#A5B4FC",
    "background": { "light": "#FFFFFF", "dark": "#0B1020" },
    "text": "#111827",
    "muted": "#4B5563",
    "border": "#D1D5DB",
    "accent": "#7C3AED",
    "success": "#047857",
    "warning": "#B45309",
    "danger": "#B91C1C"
  } }
}"##;

fn tokens(config: &liyasa_config::SiteConfig) -> Tokens {
    let theme = serde_json::to_value(&config.theme).expect("the theme slice serializes");
    let theme: ThemeConfig = serde_json::from_value(theme).expect("the theme mirror reads it");
    let (tokens, diagnostics) = Tokens::from_config(&theme);
    assert!(
        diagnostics.is_empty(),
        "the theme layer reads every colour the config validated"
    );
    tokens
}

#[test]
fn a_primary_that_fails_aa_names_the_ratio() {
    let checked = project(&with_config(
        r##"{ "name": "Acme", "seo": { "canonicalOrigin": "https://acme.dev" },
              "theme": { "colors": { "primary": "#818CF8" } } }"##,
    ));
    let contrast = one(&checked, "E0107");
    assert_eq!(
        contrast.severity,
        Severity::Warning,
        "CFG-04 warns; RFC 0102"
    );
    assert!(
        contrast.message.contains("2.98") && contrast.message.contains("4.5"),
        "the ratio and the threshold are both named: {}",
        contrast.message
    );
}

#[test]
fn every_colour_role_is_accepted() {
    let checked = project(&with_config(COLORS));
    assert_eq!(codes(&checked), Vec::<&str>::new());
}

#[test]
fn the_configured_colours_are_the_tokens() {
    let checked = project(&with_config(COLORS));
    let config = checked.config.expect("the config loads");
    let tokens = tokens(&config);

    for (token, value, scheme) in [
        ("--ly-color-primary", "#4338CA", Scheme::Light),
        ("--ly-color-primary", "#A5B4FC", Scheme::Dark),
        ("--ly-color-text", "#111827", Scheme::Light),
        ("--ly-color-border", "#D1D5DB", Scheme::Light),
        ("--ly-color-accent", "#7C3AED", Scheme::Light),
        ("--ly-color-bg", "#FFFFFF", Scheme::Light),
        ("--ly-color-bg", "#0B1020", Scheme::Dark),
    ] {
        let want = Color::parse(value).expect("the fixture's colour parses");
        assert_eq!(
            tokens.color(token, scheme),
            Some(want),
            "{token} in the {scheme:?} scheme"
        );
    }

    let css = tokens.to_css();
    let primary = Color::parse("#4338CA").expect("parses").to_string();
    assert!(
        css.contains(&format!("--ly-color-primary: {primary};")),
        "the light block carries the configured primary"
    );
}
