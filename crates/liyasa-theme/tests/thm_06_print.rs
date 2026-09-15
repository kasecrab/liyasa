//! THM-06: printing a page gives a document, not a screenshot of an
//! application.

use liyasa_theme::config::ThemeConfig;
use liyasa_theme::stylesheet::Styles;
use liyasa_theme::tokens::Tokens;

fn print_rules() -> String {
    let styles =
        Styles::build(&ThemeConfig::default(), &Tokens::aurora(), &[]).expect("the theme compiles");
    let at = styles
        .css
        .find("@media print{")
        .expect("the print stylesheet ships");
    styles.css[at..].to_owned()
}

#[test]
fn the_chrome_is_gone_when_printed() {
    let rules = print_rules();
    for chrome in [
        ".ly-navbar",
        ".ly-sidebar",
        ".ly-rail",
        ".ly-footer",
        ".ly-banner",
        ".ly-pagination",
        ".ly-feedback",
        ".ly-page-actions",
        ".ly-dialog",
        ".ly-code-copy",
    ] {
        assert!(rules.contains(chrome), "`{chrome}` is still printed");
    }
    assert!(rules.contains("display:none!important"));
}

#[test]
fn a_link_the_reader_cannot_click_shows_its_url() {
    let rules = print_rules();
    assert!(rules.contains("a[href^=http]:after"));
    assert!(rules.contains("content:\" (\" attr(href) \")\""));
    assert!(
        rules.contains("abbr[title]:after"),
        "abbreviations expand too"
    );
}

#[test]
fn nothing_breaks_across_a_page_that_should_not() {
    let rules = print_rules();
    assert!(rules.contains("break-inside:avoid"));
    assert!(rules.contains("break-after:avoid"));
    assert!(
        rules.contains("white-space:pre-wrap"),
        "code wraps rather than clips"
    );
}
