//! THM-40: every visible mark of Liyasa can be replaced or removed, and the one
//! link the theme makes to Liyasa itself comes from `liyasa_core::site`.

use liyasa_theme::context::RenderContext;
use liyasa_theme::strings::Strings;
use liyasa_theme::theme::Theme;

fn render(context: &RenderContext) -> String {
    Theme::new()
        .expect("the theme builds")
        .render_page(context)
        .expect("the page renders")
}

#[test]
fn the_built_with_line_points_at_the_published_site() {
    let context = RenderContext::sample();
    assert_eq!(context.site.built_with_url, liyasa_core::site::SITE_URL);
    let html = render(&context);
    assert!(html.contains(liyasa_core::site::SITE_URL));
    assert!(html.contains("Built with Liyasa"));
}

#[test]
fn the_oss_distribution_may_remove_the_line() {
    let mut context = RenderContext::sample();
    context.site.built_with = false;
    let html = render(&context);
    assert!(!html.contains("Built with Liyasa"));
    assert!(!html.contains(liyasa_core::site::SITE_URL));
}

#[test]
fn a_fork_replaces_the_wording_and_the_link() {
    let mut context = RenderContext::sample();
    context.site.built_with_url = "https://acme.example/platform".to_owned();
    context.strings = Strings {
        built_with: "Powered by Acme".to_owned(),
        ..Strings::default()
    };
    let html = render(&context);
    assert!(html.contains("https://acme.example/platform"));
    assert!(html.contains("Powered by Acme"));
    assert!(!html.contains(liyasa_core::site::SITE_URL));
}

#[test]
fn the_logo_favicon_and_name_are_the_operators() {
    let mut context = RenderContext::sample();
    context.site.name = "Acme docs".to_owned();
    context.site.logo.light = Some("/brand/acme.svg".to_owned());
    context.site.favicon = Some("/brand/acme.ico".to_owned());
    let html = render(&context);
    assert!(html.contains("/brand/acme.svg"));
    assert!(html.contains("/brand/acme.ico"));
    assert!(html.contains("Acme docs"));
    assert!(
        !html.contains(">Liyasa<"),
        "nothing names Liyasa but the line the operator controls"
    );
}
