//! CFG-71: the 404 page is a page of the site, and it never invites a crawler
//! to index it.

use liyasa_theme::config::ThemeConfig;
use liyasa_theme::context::{RenderContext, Site};
use liyasa_theme::strings::Strings;
use liyasa_theme::theme::Theme;

fn site() -> Site {
    RenderContext::sample().site
}

#[test]
fn the_error_page_renders_with_the_sites_chrome() {
    let context = RenderContext::not_found(site(), Strings::default());
    let html = Theme::new(&ThemeConfig::default())
        .expect("the theme builds")
        .render_page(&context)
        .expect("the 404 page renders");
    assert!(html.contains("Page not found"));
    assert!(html.contains("Back to the documentation"));
    assert!(
        html.contains("data-liyasa=\"navbar\""),
        "the chrome is the site's"
    );
    assert!(html.contains("data-liyasa=\"footer\""));
    assert!(
        !html.contains("data-liyasa=\"sidebar\""),
        "a page that does not exist has no place in the navigation"
    );
}

#[test]
fn the_error_page_is_not_indexed_and_asks_for_no_feedback() {
    let context = RenderContext::not_found(site(), Strings::default());
    let html = Theme::new(&ThemeConfig::default())
        .expect("the theme builds")
        .render_page(&context)
        .expect("the 404 page renders");
    assert!(html.contains("<meta name=\"robots\" content=\"noindex\">"));
    assert!(!html.contains("data-liyasa=\"feedback\""));
}

#[test]
fn an_operators_own_body_replaces_the_default_text() {
    let mut context = RenderContext::not_found(site(), Strings::default());
    context.page.title = "Nothing here".to_owned();
    context.set_content("<p>Try the <a href=\"/search\">search</a>.</p>");
    let html = Theme::new(&ThemeConfig::default())
        .expect("the theme builds")
        .render_page(&context)
        .expect("the 404 page renders");
    assert!(html.contains("Nothing here"));
    assert!(html.contains("Try the <a href=\"/search\">search</a>."));
    assert!(
        !html.contains("The page you are looking for does not exist."),
        "the operator's body replaces the default description"
    );
}

#[test]
fn every_string_on_it_is_rebrandable() {
    let strings = Strings {
        not_found_title: "Verdwenen".to_owned(),
        not_found_home: "Terug".to_owned(),
        ..Strings::default()
    };
    let context = RenderContext::not_found(site(), strings);
    let html = Theme::new(&ThemeConfig::default())
        .expect("the theme builds")
        .render_page(&context)
        .expect("the 404 page renders");
    assert!(html.contains("Verdwenen"));
    assert!(html.contains("Terug"));
}
