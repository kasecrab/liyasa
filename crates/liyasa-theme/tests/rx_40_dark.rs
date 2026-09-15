//! RX-40 and RX-41: the scheme is set before paint, the choice persists, a
//! strict site has no toggle, image pairs swap without a reload, and code
//! blocks change colour without re-rendering.

use liyasa_theme::config::{Appearance, Scheme, ThemeConfig};
use liyasa_theme::context::RenderContext;
use liyasa_theme::runtime::{BOOTSTRAP, Runtime};
use liyasa_theme::stylesheet::Styles;
use liyasa_theme::theme::Theme;
use liyasa_theme::tokens::{Scheme as TokenScheme, Tokens, reference};

fn render(context: &RenderContext) -> String {
    Theme::new()
        .expect("the theme builds")
        .render_page(context)
        .expect("the page renders")
}

#[test]
fn the_scheme_is_decided_before_the_stylesheet_loads() {
    let html = render(&RenderContext::sample());
    let bootstrap = html
        .find("data-liyasa=\"bootstrap\"")
        .expect("the bootstrap is inlined");
    let head_end = html.find("</head>").expect("there is a head");
    assert!(bootstrap < head_end, "the bootstrap runs in the head");
    assert!(html.contains("data-ly-appearance=\"system\""));
    assert!(BOOTSTRAP.contains("setAttribute(\"data-theme\""));
    assert!(
        BOOTSTRAP.contains("removeAttribute(\"data-theme\")"),
        "a system reader keeps the media query rather than a pinned attribute"
    );
}

#[test]
fn the_inline_bootstrap_carries_the_nonce() {
    let mut context = RenderContext::sample();
    context.nonce = "abc123".to_owned();
    let html = render(&context);
    assert!(html.contains("<script nonce=\"abc123\" data-liyasa=\"bootstrap\">"));
    assert!(html.contains("<style nonce=\"abc123\" data-liyasa=\"critical\">"));
}

#[test]
fn a_strict_site_ships_neither_the_toggle_nor_its_module() {
    let config = ThemeConfig {
        appearance: Appearance {
            default: Scheme::Dark,
            strict: true,
            ..Appearance::default()
        },
        ..ThemeConfig::default()
    };
    let mut context = RenderContext::sample();
    context.site.appearance.strict = true;
    context.site.appearance.default = "dark".to_owned();
    let html = render(&context);
    assert!(!html.contains("data-ly-theme-toggle"));
    assert!(html.contains("data-ly-appearance-strict=\"true\""));
    assert!(html.contains("data-theme=\"dark\""));
    assert!(
        !Runtime::build(&config)
            .base
            .contains("data-ly-theme-toggle")
    );
}

#[test]
fn the_choice_persists_and_wins_over_the_system_preference() {
    assert!(BOOTSTRAP.contains("localStorage.getItem(\"liyasa:theme\")"));
    let css = Tokens::aurora().to_css();
    let media = css
        .find("@media (prefers-color-scheme: dark)")
        .expect("media block");
    let attribute = css
        .find("\n[data-theme=\"dark\"]")
        .expect("attribute block");
    assert!(attribute > media);
}

#[test]
fn an_image_pair_swaps_without_a_reload() {
    let styles = Styles::build(&ThemeConfig::default(), &Tokens::aurora(), &[]);
    assert!(
        styles
            .css
            .contains("[data-ly-scheme=\"dark\"]{display:none}")
    );
    assert!(
        styles
            .css
            .contains("[data-theme=\"dark\"] [data-ly-scheme=\"dark\"]")
    );
    assert!(
        styles
            .css
            .contains("[data-theme=\"dark\"] [data-ly-scheme=\"light\"]")
    );

    let html = render(&RenderContext::sample());
    assert!(html.contains("data-ly-scheme=\"light\""));
    assert!(html.contains("data-ly-scheme=\"dark\""));
}

#[test]
fn both_code_themes_are_emitted_as_variables() {
    let tokens = Tokens::aurora();
    let code_tokens: Vec<&str> = reference()
        .iter()
        .map(|spec| spec.name)
        .filter(|name| name.starts_with("--ly-code-token-"))
        .collect();
    assert!(
        code_tokens.len() >= 10,
        "the code theme has a full token set"
    );
    for name in code_tokens {
        let light = tokens.get(name, TokenScheme::Light).expect("a light value");
        let dark = tokens.get(name, TokenScheme::Dark).expect("a dark value");
        assert_ne!(light, dark, "`{name}` is tuned per scheme");
    }

    // Every highlighted span reads a variable, so switching the scheme repaints
    // rather than re-rendering: no markup depends on the current scheme.
    let styles = Styles::build(&ThemeConfig::default(), &Tokens::aurora(), &[]);
    assert!(
        styles
            .css
            .contains(".ly-tok-keyword{color:var(--ly-code-token-keyword)}")
    );
    assert!(
        styles
            .css
            .contains(".ly-tok-string{color:var(--ly-code-token-string)}")
    );
}
