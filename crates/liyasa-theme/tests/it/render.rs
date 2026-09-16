//! The page assembles, and every mode renders (RX-01, RX-20..23, THM-21).

use liyasa_theme::context::{Mode, RenderContext};
use liyasa_theme::theme::Theme;

fn theme() -> Theme {
    Theme::new().expect("the default theme builds")
}

#[test]
fn the_default_page_carries_its_content_and_chrome() {
    let context = RenderContext::sample();
    let html = theme().render_page(&context).expect("the page renders");
    assert!(html.starts_with("<!doctype html>"));
    assert!(html.contains("<main class=\"ly-main\""));
    assert!(html.contains("<p>Body</p>"), "the content is in the HTML");
    assert!(html.contains("data-liyasa=\"sidebar\""));
    assert!(html.contains("data-liyasa=\"toc\""));
    assert!(html.contains("data-liyasa=\"footer\""));
    assert!(html.contains("Install"), "the title is rendered");
}

#[test]
fn every_built_in_mode_has_a_layout() {
    let theme = theme();
    for name in Mode::BUILT_IN {
        let mut context = RenderContext::sample();
        context.page.mode = Mode::parse(name);
        let html = theme
            .render_page(&context)
            .unwrap_or_else(|error| panic!("`{name}` renders: {error}"));
        assert!(html.contains("<p>Body</p>"), "`{name}` lost the content");
    }
}
