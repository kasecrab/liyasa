use liyasa_core::diagnostics::Severity;

use super::*;
use crate::core::prose::{Linter, Passage, Scope};
use crate::core::spell::{Dictionary, SpellChecker};

fn linter() -> Linter {
    Linter::new(liyasa())
}

fn para(text: &str) -> Passage {
    Passage {
        block: liyasa_core::ids::BlockId::explicit(text),
        span: None,
        scope: Scope::Paragraph,
        text: text.to_owned(),
    }
}

fn heading(text: &str) -> Passage {
    Passage {
        scope: Scope::Heading,
        ..para(text)
    }
}

fn rules_that_fired(passages: &[Passage]) -> Vec<String> {
    let mut out: Vec<String> = linter()
        .check("a.md", passages, None)
        .into_iter()
        .map(|f| f.rule)
        .collect();
    out.sort_unstable();
    out.dedup();
    out
}

#[test]
fn every_bundled_rule_parses() {
    for (name, source) in RULES {
        Rule::parse(name, source).unwrap_or_else(|error| panic!("{name}: {error}"));
    }
    assert_eq!(liyasa().len(), RULES.len());
}

#[test]
fn every_bundled_rule_is_one_liyasa_can_run() {
    for rule in liyasa() {
        assert!(
            rule.is_supported(),
            "{} would have to go to the Vale binary",
            rule.name
        );
    }
}

#[test]
fn every_bundled_rule_is_named_for_the_style_it_belongs_to() {
    for (name, _) in RULES {
        assert!(name.starts_with("Liyasa."), "{name}");
    }
    assert!(
        RULES.windows(2).all(|w| w[0].0 < w[1].0),
        "the list is sorted, so a new rule has one place to go"
    );
}

#[test]
fn the_weasel_rule_finds_a_weasel_word() {
    assert!(
        rules_that_fired(&[para("The build is very fast.")]).contains(&"Liyasa.Weasel".to_owned())
    );
}

#[test]
fn the_terms_rule_suggests_the_shorter_word() {
    let found = linter().check(
        "a.md",
        &[para("Utilize the cache in order to build.")],
        None,
    );
    let messages: Vec<&str> = found.iter().map(|f| f.message.as_str()).collect();
    assert!(messages.iter().any(|m| m.contains("'use'")), "{messages:?}");
    assert!(messages.iter().any(|m| m.contains("'to'")), "{messages:?}");
}

#[test]
fn the_inclusive_rule_is_an_error_not_a_warning() {
    let found = linter().check("a.md", &[para("Add the host to the whitelist.")], None);
    let inclusive: Vec<_> = found
        .iter()
        .filter(|f| f.rule == "Liyasa.Inclusive")
        .collect();
    assert_eq!(inclusive.len(), 1, "{found:?}");
    assert_eq!(inclusive[0].severity, Severity::Error);
    assert!(
        inclusive[0].message.contains("allow list"),
        "{}",
        inclusive[0].message
    );
}

#[test]
fn the_headings_rule_wants_sentence_case() {
    assert!(
        rules_that_fired(&[heading("Getting Started With The Cache")])
            .contains(&"Liyasa.Headings".to_owned())
    );
}

#[test]
fn the_headings_rule_leaves_the_product_names_alone() {
    let found = linter().check(
        "a.md",
        &[heading("Writing Markdown for Liyasa with OpenAPI")],
        None,
    );
    assert!(
        !found.iter().any(|f| f.rule == "Liyasa.Headings"),
        "{found:?}"
    );
}

#[test]
fn the_consistency_rule_fires_only_on_both_spellings() {
    assert!(rules_that_fired(&[para("the colour")]).is_empty());
    assert!(
        rules_that_fired(&[para("the colour"), para("the color")])
            .contains(&"Liyasa.Consistency".to_owned())
    );
}

#[test]
fn the_latin_rule_is_only_a_suggestion() {
    let found = linter().check("a.md", &[para("Any format, e.g. JSON.")], None);
    let latin: Vec<_> = found.iter().filter(|f| f.rule == "Liyasa.Latin").collect();
    assert_eq!(latin.len(), 1, "{found:?}");
    assert_eq!(latin[0].severity, Severity::Hint);
}

#[test]
fn the_exclamation_rule_reads_prose_and_not_code() {
    assert!(rules_that_fired(&[para("It works!")]).contains(&"Liyasa.Exclamation".to_owned()));
    let code = Passage {
        scope: Scope::Code,
        ..para("let ok = !failed;")
    };
    assert!(rules_that_fired(&[code]).is_empty());
}

#[test]
fn the_spelling_rule_uses_whatever_dictionary_it_is_handed() {
    let speller = SpellChecker::new(Dictionary::from_lines("the\nbuild\nis\ndone\n"));
    let found = linter().check("a.md", &[para("The buidl is done.")], Some(&speller));
    assert!(
        found
            .iter()
            .any(|f| f.rule == "Liyasa.Spelling" && f.found == "buidl"),
        "{found:?}"
    );
}

#[test]
fn a_clean_paragraph_trips_no_bundled_rule() {
    let speller = SpellChecker::new(Dictionary::from_lines(
        "liyasa\nbuilds\nthe\nsite\nfrom\nmarkdown\nand\nchecks\nevery\nlink\n",
    ));
    let found = linter().check(
        "a.md",
        &[para(
            "Liyasa builds the site from Markdown and checks every link.",
        )],
        Some(&speller),
    );
    assert!(found.is_empty(), "{found:?}");
}

#[test]
fn the_bundled_style_can_be_turned_off_from_vale_ini() {
    let ini = crate::core::prose::ini::ValeIni::parse(
        "[*.md]\nBasedOnStyles = Liyasa\nLiyasa.Weasel = NO\n",
    );
    let found = linter()
        .with_ini(ini)
        .check("a.md", &[para("It is very fast.")], None);
    assert!(
        !found.iter().any(|f| f.rule == "Liyasa.Weasel"),
        "{found:?}"
    );
}
