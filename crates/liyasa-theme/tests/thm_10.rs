//! THM-10..12: tokens are generated from `theme.*`, an operator overrides any
//! of them in `theme/tokens.css`, and one override reaches both schemes.

use liyasa_theme::config::{Colors, ThemeConfig};
use liyasa_theme::stylesheet::Styles;
use liyasa_theme::tokens::{Group, Scheme, Tokens, reference};

#[test]
fn an_override_applies_in_both_schemes() {
    let mut tokens = Tokens::aurora();
    tokens.with_overrides(":root { --ly-color-primary: #7c3aed; }");

    assert_eq!(
        tokens.get("--ly-color-primary", Scheme::Light),
        Some("#7c3aed")
    );
    assert_eq!(
        tokens.get("--ly-color-primary", Scheme::Dark),
        Some("#7c3aed")
    );

    let styles = Styles::build(&ThemeConfig::default(), &tokens, &[]).expect("the theme compiles");
    assert_eq!(
        styles.css.matches("--ly-color-primary:#7c3aed").count(),
        1,
        "a token with one value is emitted once, in :root"
    );
}

#[test]
fn a_scheme_scoped_override_touches_only_that_scheme() {
    let mut tokens = Tokens::aurora();
    let before = tokens
        .get("--ly-color-bg", Scheme::Light)
        .map(ToOwned::to_owned);
    tokens.with_overrides(
        r#"
        [data-theme="dark"] { --ly-color-bg: #000000; }
        @media (prefers-color-scheme: dark) { :root { --ly-color-text: #ffffff; } }
        "#,
    );

    // An override's value is minified with the rest of the sheet, so the token
    // carries the short form the stylesheet will.
    assert_eq!(tokens.get("--ly-color-bg", Scheme::Dark), Some("#000"));
    assert_eq!(
        tokens.get("--ly-color-bg", Scheme::Light),
        before.as_deref()
    );
    assert_eq!(tokens.get("--ly-color-text", Scheme::Dark), Some("#fff"));
}

#[test]
fn an_operators_own_token_is_kept() {
    let mut tokens = Tokens::aurora();
    tokens.with_overrides(":root { --brand-hero-height: 32rem; }");
    assert_eq!(
        tokens.get("--brand-hero-height", Scheme::Light),
        Some("32rem")
    );
    let styles = Styles::build(&ThemeConfig::default(), &tokens, &[]).expect("the theme compiles");
    assert!(styles.css.contains("--brand-hero-height:32rem"));
}

#[test]
fn the_token_list_matches_the_reference() {
    let tokens = Tokens::aurora();
    let names = tokens.names();
    assert_eq!(names.len(), reference().len());
    for spec in reference() {
        assert!(
            names.contains(&spec.name),
            "`{}` is documented but not emitted",
            spec.name
        );
        assert!(
            spec.name.starts_with("--ly-"),
            "`{}` does not carry the theme's prefix",
            spec.name
        );
    }
}

#[test]
fn every_group_thm_11_names_has_tokens() {
    for group in Group::ALL {
        let count = reference()
            .iter()
            .filter(|spec| spec.group == group)
            .count();
        assert!(count > 0, "{} has no tokens", group.title());
    }
}

#[test]
fn config_and_override_compose_with_the_override_last() {
    let config = ThemeConfig {
        colors: Colors {
            primary: Some("#005f73".to_owned()),
            ..Colors::default()
        },
        ..ThemeConfig::default()
    };
    let (mut tokens, diagnostics) = Tokens::from_config(&config);
    assert!(diagnostics.is_empty());
    tokens.with_overrides(":root { --ly-color-primary: #b91c1c; }");
    assert_eq!(
        tokens.get("--ly-color-primary", Scheme::Light),
        Some("#b91c1c")
    );
}
