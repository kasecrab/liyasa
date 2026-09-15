use super::*;

fn checker() -> SpellChecker {
    SpellChecker::new(Dictionary::from_lines(
        "the\nquick\nbrown\nfox\njumps\nover\nlazy\ndog\nliyasa\ndocumentation\nbuild\nrunner\npage\n",
    ))
}

#[test]
fn a_word_in_the_dictionary_passes() {
    assert_eq!(checker().check("the quick brown fox"), Vec::new());
}

#[test]
fn a_word_that_is_not_is_reported_with_its_offset() {
    let found = checker().check("the quikc brown fox");
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].word, "quikc");
    assert_eq!(found[0].at, 4);
    assert_eq!(found[0].diagnostic().code, code::W0632);
}

#[test]
fn the_dictionary_is_case_insensitive() {
    assert_eq!(checker().check("The Quick Brown Fox"), Vec::new());
}

#[test]
fn a_plural_or_possessive_of_a_known_word_is_known() {
    assert_eq!(checker().check("the runner's pages"), Vec::new());
    assert_eq!(checker().check("the runner\u{2019}s pages"), Vec::new());
}

#[test]
fn an_ignored_word_passes_whatever_the_dictionary_says() {
    let checker = checker().ignoring(["kubernetes"]);
    assert_eq!(checker.check("the kubernetes page"), Vec::new());
}

// ---- code-aware tokenization (the point of VER-62) ----

#[test]
fn code_shaped_tokens_are_not_words() {
    let checker = SpellChecker::new(Dictionary::new());
    let code = [
        "serde_json",
        "SCREAMING_CASE",
        "std::collections::BTreeMap",
        "kubectl", // no: a bare lowercase word really is a word
        "x86_64",
        "install.md",
        "facts.pricing.pro",
        "https://example.com/a",
        "www.example.com",
        "someone@example.com",
        "src/main.rs",
        "C:\\Windows",
        "format()",
        "0.11.3",
        "blake3",
        "camelCase",
        "JsonValue",
        "HTTP/2",
    ];
    for token in code {
        let found = checker.check(token);
        if token == "kubectl" {
            assert_eq!(found.len(), 1, "a bare lowercase word is prose");
            continue;
        }
        assert!(found.is_empty(), "{token} was read as prose: {found:?}");
    }
}

#[test]
fn an_ordinary_capitalized_word_is_still_prose() {
    let checker = SpellChecker::new(Dictionary::from_lines("liyasa\nlondon\nin\n"));
    assert_eq!(checker.check("Liyasa in London"), Vec::new());
    assert_eq!(checker.check("Liyasa in Lundon").len(), 1);
}

#[test]
fn an_acronym_is_prose_not_code() {
    let checker = SpellChecker::new(Dictionary::from_lines("the\nhttp\napi\n"));
    assert_eq!(checker.check("the HTTP API"), Vec::new());
    assert_eq!(checker.check("the HTTP XPI").len(), 1);
}

#[test]
fn a_hyphenated_compound_is_spelled_a_part_at_a_time() {
    let checker = SpellChecker::new(Dictionary::from_lines("well\nknown\n"));
    assert_eq!(
        checker.check("a well-known thing").len(),
        1,
        "`thing` is missing"
    );
    let found = checker.check("a wel-known thing");
    assert_eq!(found.len(), 2);
    assert_eq!(found[0].word, "wel");
}

#[test]
fn a_hyphenated_part_keeps_a_usable_offset() {
    let checker = SpellChecker::new(Dictionary::from_lines("well\n"));
    let found = checker.check("a well-knwon thing");
    assert_eq!(found[0].word, "knwon");
    assert_eq!(&"a well-knwon thing"[found[0].at..found[0].at + 5], "knwon");
}

#[test]
fn sentence_punctuation_is_not_part_of_a_word() {
    let checker = SpellChecker::new(Dictionary::from_lines("done\nyes\nno\n"));
    assert_eq!(checker.check("Done. Yes, no; \"done\" (done)!"), Vec::new());
}

#[test]
fn a_single_letter_is_not_spelled() {
    let checker = SpellChecker::new(Dictionary::new());
    assert_eq!(checker.check("a b c I"), Vec::new());
}

#[test]
fn an_empty_text_finds_nothing() {
    assert_eq!(checker().check(""), Vec::new());
    assert_eq!(checker().check("   \n  "), Vec::new());
}

// ---- the dictionary file format ----

#[test]
fn a_dictionary_skips_blanks_and_comments() {
    let dictionary = Dictionary::from_lines("# a comment\n\nliyasa\n  comrak  \nvale # inline\n");
    assert_eq!(dictionary.len(), 3);
    assert!(dictionary.contains("liyasa"));
    assert!(dictionary.contains("Comrak"));
    assert!(dictionary.contains("vale"));
    assert!(!dictionary.contains("a"));
}

#[test]
fn an_empty_dictionary_is_empty() {
    assert!(Dictionary::new().is_empty());
    assert!(Dictionary::from_lines("\n# only comments\n").is_empty());
}

#[test]
fn words_reports_offsets_into_the_text_it_was_given() {
    let text = "alpha beta gamma";
    let found = words(text);
    for (word, at) in &found {
        assert_eq!(&text[*at..*at + word.len()], word);
    }
    assert_eq!(found.len(), 3);
}
