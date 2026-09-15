//! RX-01 and CMP-102: the HTML carries the whole page, no content script
//! survives, and nothing is left for the client to render.

use liyasa_theme::config::ThemeConfig;
use liyasa_theme::context::RenderContext;
use liyasa_theme::theme::Theme;

const MARKDOWN: &str = "\
# Install

Liyasa builds documentation from Markdown files and a configuration file.

## Requirements

A supported platform and a terminal. Nothing else is needed to build a site.

## Next steps

Read the configuration reference, then write the first page.
";

/// The rendered body as plain text, which is what a reader without JavaScript
/// and what an agent fetching the page both see.
fn text_of(html: &str) -> String {
    let main = html
        .split("<main")
        .nth(1)
        .and_then(|rest| rest.split("</main>").next())
        .unwrap_or(html);
    let mut out = String::with_capacity(main.len());
    let mut inside_tag = false;
    for character in main.chars() {
        match character {
            '<' => inside_tag = true,
            '>' => {
                inside_tag = false;
                out.push(' ');
            }
            other if !inside_tag => out.push(other),
            _ => {}
        }
    }
    out
}

fn words(text: &str) -> Vec<String> {
    text.split_whitespace()
        .map(|word| {
            word.trim_matches(|c: char| !c.is_alphanumeric())
                .to_lowercase()
        })
        .filter(|word| !word.is_empty())
        .collect()
}

fn rendered() -> String {
    // What `liyasa-markdown` would produce for MARKDOWN; the theme's job is to
    // carry it into the page unchanged.
    let content = "<h1>Install</h1>\n<p>Liyasa builds documentation from Markdown files and a \
                   configuration file.</p>\n<h2 id=\"requirements\">Requirements</h2>\n<p>A \
                   supported platform and a terminal. Nothing else is needed to build a \
                   site.</p>\n<h2 id=\"next-steps\">Next steps</h2>\n<p>Read the configuration \
                   reference, then write the first page.</p>";
    let mut context = RenderContext::sample();
    context.set_content(content);
    Theme::new(&ThemeConfig::default())
        .expect("the theme builds")
        .render_page(&context)
        .expect("the page renders")
}

#[test]
fn the_html_carries_the_whole_page() {
    let html = rendered();
    let rendered_words = words(&text_of(&html));
    let source_words: Vec<String> = words(MARKDOWN)
        .into_iter()
        .filter(|word| word != "#" && word != "##")
        .collect();
    let present = source_words
        .iter()
        .filter(|word| rendered_words.contains(word))
        .count();
    let parity = present as f32 / source_words.len() as f32;
    assert!(
        parity > 0.95,
        "text parity with the Markdown output is {parity:.2}"
    );
}

#[test]
fn nothing_is_left_for_the_client_to_render() {
    let html = rendered();
    // No hydration payload, no serialized component tree (RX-14).
    assert!(!html.contains("__NEXT_DATA__"));
    assert!(!html.contains("hydrate"));
    let page_data = html
        .split("id=\"ly-page-data\"")
        .nth(1)
        .and_then(|rest| rest.split("</script>").next())
        .expect("the page data element renders");
    assert!(
        page_data.len() < 4096,
        "the page data is metadata, not a copy of the page"
    );
    assert!(
        !html.contains("<script>document.write"),
        "content is never written by script"
    );
}

#[test]
fn every_script_the_page_loads_is_deferred_and_same_origin() {
    let html = rendered();
    for (at, _) in html.match_indices("<script src=") {
        let tag = &html[at..html[at..].find('>').map_or(html.len(), |end| at + end)];
        assert!(tag.contains(" defer"), "`{tag}` blocks the parser");
        assert!(
            !tag.contains("//") || tag.contains("src=\"/"),
            "`{tag}` loads from another origin"
        );
    }
}

#[test]
fn a_script_in_content_never_reaches_the_page() {
    let mut context = RenderContext::sample();
    let diagnostics = context.set_content(
        "<p>before</p><script>fetch('/steal')</script><img src=\"x\" onerror=\"go()\" alt=\"x\">",
    );
    assert_eq!(diagnostics.len(), 2);
    for diagnostic in &diagnostics {
        assert_eq!(diagnostic.code.as_str(), "E0304");
    }
    let html = Theme::new(&ThemeConfig::default())
        .expect("the theme builds")
        .render_page(&context)
        .expect("the page renders");
    assert!(html.contains("<p>before</p>"));
    assert!(!html.contains("fetch('/steal')"));
    assert!(!html.contains("onerror"));
}

#[test]
fn the_page_is_readable_with_javascript_disabled() {
    let html = rendered();
    // Everything a reader needs is markup: the content, the navigation, the
    // table of contents, and a search entry point.
    assert!(html.contains("<h1>Install</h1>"));
    assert!(html.contains("data-liyasa=\"sidebar\""));
    assert!(html.contains("data-liyasa=\"toc\""));
    assert!(html.contains("<noscript>"));
    // Elements that do nothing without their module start hidden, so a reader
    // without JavaScript is never shown a control that cannot work.
    for control in [
        "data-ly-theme-toggle",
        "data-ly-search-trigger",
        "data-ly-back-to-top",
    ] {
        let at = html.find(control).expect("the control renders");
        let tag = &html[at..html[at..].find('>').map_or(html.len(), |end| at + end)];
        assert!(
            tag.contains("hidden"),
            "`{control}` is offered without its module"
        );
    }
}
