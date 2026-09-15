//! `spec/markdown/cm-40/abbreviations/`.

use liyasa_core::document::BlockKind;

use crate::directives::testing::*;

fn expanded_in(source: &str) -> Vec<(String, String)> {
    let document = document(source);
    inlines(&document.root)
        .into_iter()
        .filter_map(|inline| match inline {
            liyasa_core::document::Inline::InlineComponent {
                name,
                props,
                children,
            } if name == super::ABBR => Some((
                crate::ast::build::plain(children),
                match props.get("title") {
                    Some(liyasa_core::document::PropValue::Str(title)) => title.clone(),
                    _ => String::new(),
                },
            )),
            _ => None,
        })
        .collect()
}

#[test]
fn a_definition_expands_every_occurrence() {
    assert_eq!(
        expanded_in("HTML is a language. So is HTML.\n\n*[HTML]: HyperText Markup Language\n"),
        [
            ("HTML".to_owned(), "HyperText Markup Language".to_owned()),
            ("HTML".to_owned(), "HyperText Markup Language".to_owned()),
        ]
    );
}

#[test]
fn a_definition_produces_no_block_of_its_own() {
    let document = document("x\n\n*[HTML]: HyperText Markup Language\n");
    let paragraphs = blocks(&document.root)
        .into_iter()
        .filter(|b| matches!(b.kind, BlockKind::Paragraph))
        .count();
    assert_eq!(paragraphs, 1);
    assert_eq!(codes(&document), Vec::<&str>::new());
}

#[test]
fn a_definition_anywhere_on_the_page_applies() {
    assert_eq!(
        expanded_in("*[HTML]: HyperText Markup Language\n\nHTML here.\n").len(),
        1
    );
}

#[test]
fn only_whole_words_match() {
    assert!(expanded_in("HTMLish\n\n*[HTML]: HyperText\n").is_empty());
    assert!(expanded_in("xHTML\n\n*[HTML]: HyperText\n").is_empty());
    assert_eq!(expanded_in("(HTML)\n\n*[HTML]: HyperText\n").len(), 1);
}

#[test]
fn matching_is_case_sensitive() {
    assert!(expanded_in("html\n\n*[HTML]: HyperText\n").is_empty());
}

#[test]
fn the_longest_definition_wins() {
    assert_eq!(
        expanded_in("HTML5 rules.\n\n*[HTML]: HyperText\n*[HTML5]: HyperText 5\n"),
        [("HTML5".to_owned(), "HyperText 5".to_owned())]
    );
}

#[test]
fn a_code_span_and_a_code_block_are_not_prose() {
    assert!(expanded_in("`HTML`\n\n*[HTML]: HyperText\n").is_empty());
    assert!(expanded_in("```\nHTML\n```\n\n*[HTML]: HyperText\n").is_empty());
}

#[test]
fn an_abbreviation_inside_emphasis_is_found() {
    assert_eq!(expanded_in("*HTML*\n\n*[HTML]: HyperText\n").len(), 1);
}

#[test]
fn a_page_with_no_definitions_is_untouched() {
    let document = document("HTML is a language.\n");
    assert_eq!(
        inlines(&document.root),
        [&liyasa_core::document::Inline::Text(
            "HTML is a language.".to_owned()
        )]
    );
}

/// The definitions must survive a round trip, or the second build loses them.
#[test]
fn definitions_are_written_back() {
    let source = "HTML is a language.\n\n*[HTML]: HyperText Markup Language\n";
    let once = crate::render::markdown::render(&document(source).root);
    assert_eq!(once, source);
    assert_eq!(crate::render::markdown::render(&document(&once).root), once);
}

/// A paragraph holds as many matches as it has words; one stack frame each
/// would end the process rather than the page.
#[test]
fn a_page_full_of_matches_does_not_overflow_the_stack() {
    let prose = "A ".repeat(50_000);
    let ampere = "Ampere".to_owned();
    let abbr = "A".to_owned();
    let out = super::split(&prose, &[(&abbr, &ampere)]);
    // Every match also leaves the space after it, so the run is twice as long
    // as the number of abbreviations.
    assert_eq!(out.len(), 100_000);
}
