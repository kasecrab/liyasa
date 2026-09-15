//! RX-23: the right rail holds the table of contents, and a `:::panel`
//! replaces its contents.

use liyasa_theme::config::ThemeConfig;
use liyasa_theme::context::{Mode, RenderContext};
use liyasa_theme::theme::Theme;

fn render(context: &RenderContext) -> String {
    Theme::new(&ThemeConfig::default())
        .expect("the theme builds")
        .render_page(context)
        .expect("the page renders")
}

#[test]
fn the_rail_holds_the_table_of_contents_by_default() {
    let html = render(&RenderContext::sample());
    assert!(html.contains("data-liyasa=\"rail\""));
    assert!(html.contains("data-liyasa=\"toc\""));
    assert!(!html.contains("data-liyasa=\"panel\""));
}

#[test]
fn a_panel_replaces_the_table_of_contents() {
    let mut context = RenderContext::sample();
    let diagnostics = context.set_panel("<p>Ask about this endpoint</p>");
    assert!(diagnostics.is_empty());
    let html = render(&context);
    assert!(html.contains("data-liyasa=\"panel\""));
    assert!(html.contains("Ask about this endpoint"));
    assert!(
        !html.contains("data-liyasa=\"toc\""),
        "the panel replaces the rail's contents rather than joining them"
    );
}

#[test]
fn a_panel_is_stripped_like_any_other_content() {
    let mut context = RenderContext::sample();
    let diagnostics = context.set_panel("<p>ok</p><script>alert(1)</script>");
    assert_eq!(diagnostics.len(), 1);
    let html = render(&context);
    assert!(html.contains("<p>ok</p>"));
    assert!(!html.contains("alert(1)"));
}

#[test]
fn wide_and_custom_modes_drop_the_rail() {
    for mode in [Mode::Wide, Mode::Custom, Mode::Center, Mode::Frame] {
        let mut context = RenderContext::sample();
        context.page.mode = mode.clone();
        let html = render(&context);
        assert!(
            !html.contains("data-liyasa=\"rail\""),
            "`{}` still rendered the rail",
            mode.name()
        );
    }
}

#[test]
fn only_the_modes_that_declare_a_sidebar_render_one() {
    for mode in [Mode::Default, Mode::Wide, Mode::Assistant] {
        let mut context = RenderContext::sample();
        context.page.mode = mode.clone();
        assert!(
            render(&context).contains("data-liyasa=\"sidebar\""),
            "`{}` should have a sidebar",
            mode.name()
        );
    }
    for mode in [Mode::Custom, Mode::Center, Mode::Frame] {
        let mut context = RenderContext::sample();
        context.page.mode = mode.clone();
        assert!(
            !render(&context).contains("data-liyasa=\"sidebar\""),
            "`{}` should not have a sidebar",
            mode.name()
        );
    }
}
