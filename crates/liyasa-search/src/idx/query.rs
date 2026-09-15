//! The query language (SRC-04): prefixes, fuzziness, phrases, and filters.
//!
//! Parsed once and run by both halves, so `version:v2 "rate limit"` means the
//! same thing in the browser, on the server, over REST, and in the CLI.

use super::tokenize::Tokenizer;
use crate::doc::DocKind;
use crate::error::SearchError;

/// Below this length a typo is as likely to be a different word, so fuzzy
/// matching does more harm than good (SRC-04: "terms over 4 chars").
pub const FUZZY_MIN_LENGTH: usize = 5;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Term {
    /// The stemmed form, which is how the prose fields were indexed.
    pub text: String,
    /// Forms that mean the same term in another field. The `code` field is
    /// never stemmed (SRC-02), so a query for `getUser` must also look up
    /// `getuser` or it misses the symbol it names.
    pub alternatives: Vec<String>,
    /// The last term of a query still being typed also matches by prefix.
    pub prefix: bool,
    /// Long enough to tolerate one edit.
    pub fuzzy: bool,
}

impl Term {
    /// The term and its alternatives, best first.
    pub fn forms(&self) -> impl Iterator<Item = &str> {
        std::iter::once(self.text.as_str()).chain(self.alternatives.iter().map(String::as_str))
    }
}

/// The facets RX-32 filters on, plus the reader's own scope.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Filters {
    pub tab: Option<String>,
    pub version: Option<String>,
    pub locale: Option<String>,
    pub kind: Option<DocKind>,
}

impl Filters {
    pub const NAMES: [&'static str; 4] = ["tab", "version", "locale", "type"];

    pub fn is_empty(&self) -> bool {
        self == &Self::default()
    }

    /// Sets one filter by name, rejecting a facet that does not exist rather
    /// than silently returning everything.
    pub fn set(&mut self, name: &str, value: &str) -> Result<(), SearchError> {
        match name {
            "tab" => self.tab = Some(value.to_owned()),
            "version" => self.version = Some(value.to_owned()),
            "locale" => self.locale = Some(value.to_owned()),
            "type" => {
                self.kind = Some(DocKind::parse(value).ok_or_else(|| {
                    SearchError::Query(format!(
                        "`{value}` is not a content type; expected page, endpoint, or changelog"
                    ))
                })?);
            }
            other => {
                return Err(SearchError::Query(format!(
                    "`{other}` is not a filter; expected one of {}",
                    Self::NAMES.join(", ")
                )));
            }
        }
        Ok(())
    }
}

/// Who is asking. A document with groups or regions is invisible to a reader
/// outside them, before ranking rather than after (SRC-06).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReaderScope {
    pub groups: Vec<String>,
    pub region: Option<String>,
}

impl ReaderScope {
    pub fn admits(&self, groups: &[String], regions: &[String]) -> bool {
        let in_groups = groups.is_empty()
            || groups
                .iter()
                .any(|g| self.groups.iter().any(|own| own == g));
        let in_regions = regions.is_empty()
            || self
                .region
                .as_ref()
                .is_some_and(|region| regions.iter().any(|r| r == region));
        in_groups && in_regions
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Query {
    pub terms: Vec<Term>,
    /// Each quoted run, already tokenized. A phrase's terms also count as
    /// ordinary terms, so a phrase narrows rather than replaces.
    pub phrases: Vec<Vec<String>>,
    pub filters: Filters,
    /// What the reader typed, for analytics and for the empty-query case.
    pub raw: String,
}

impl Query {
    pub fn is_empty(&self) -> bool {
        self.terms.is_empty()
    }
}

/// Parses `input` as the reader's locale tokenizes it.
///
/// `word:value` is a filter only for the four facet names; anything else is
/// ordinary text, so searching for `note: see below` finds the note.
pub fn parse(input: &str, locale: &str) -> Result<Query, SearchError> {
    let tokenizer = Tokenizer::for_locale(locale);
    let mut query = Query {
        raw: input.to_owned(),
        ..Query::default()
    };
    let mut seen: Vec<String> = Vec::new();

    let pieces = split(input)?;
    let last = pieces.len();
    for (n, piece) in pieces.into_iter().enumerate() {
        match piece {
            Piece::Phrase(text) => {
                let terms: Vec<String> = tokenizer
                    .tokenize(&text)
                    .into_iter()
                    .map(|t| t.text)
                    .collect();
                for term in &terms {
                    push_term(&mut query, &mut seen, term, false, term.chars().count());
                }
                if terms.len() > 1 {
                    query.phrases.push(terms);
                }
            }
            Piece::Filter { name, value } => query.filters.set(&name, &value)?,
            Piece::Word(text) => {
                // Only the word the reader is still typing matches by prefix.
                let typing = n + 1 == last;
                let unstemmed = text.to_lowercase();
                // Fuzziness is judged on what the reader typed, not on its
                // stem: `limts` is a five-letter typo whose stem is four.
                let typed = text.chars().count();
                for token in tokenizer.tokenize(&text) {
                    let at = push_term(&mut query, &mut seen, &token.text, typing, typed);
                    if let Some(term) = at.and_then(|at| query.terms.get_mut(at))
                        && term.text != unstemmed
                        && !term.alternatives.contains(&unstemmed)
                    {
                        term.alternatives.push(unstemmed.clone());
                    }
                }
            }
        }
    }
    Ok(query)
}

/// Adds a term, or widens the one already there, and returns its index.
fn push_term(
    query: &mut Query,
    seen: &mut Vec<String>,
    text: &str,
    prefix: bool,
    typed_length: usize,
) -> Option<usize> {
    if text.is_empty() {
        return None;
    }
    if let Some(at) = seen.iter().position(|t| t == text) {
        // A term repeated between a phrase and a bare word keeps the widest
        // matching it was asked for.
        if let Some(term) = query.terms.get_mut(at) {
            term.prefix |= prefix;
        }
        return Some(at);
    }
    seen.push(text.to_owned());
    query.terms.push(Term {
        text: text.to_owned(),
        alternatives: Vec::new(),
        prefix,
        fuzzy: typed_length.max(text.chars().count()) >= FUZZY_MIN_LENGTH,
    });
    Some(query.terms.len() - 1)
}

enum Piece {
    Word(String),
    Phrase(String),
    Filter { name: String, value: String },
}

/// Splits on whitespace, keeping quoted runs together.
fn split(input: &str) -> Result<Vec<Piece>, SearchError> {
    let mut pieces = Vec::new();
    let mut current = String::new();
    let mut quoted = false;

    let flush = |current: &mut String, pieces: &mut Vec<Piece>| {
        if current.is_empty() {
            return;
        }
        let word = std::mem::take(current);
        match word.split_once(':') {
            Some((name, value))
                if !value.is_empty()
                    && Filters::NAMES.contains(&name.to_ascii_lowercase().as_str()) =>
            {
                pieces.push(Piece::Filter {
                    name: name.to_ascii_lowercase(),
                    value: value.to_owned(),
                });
            }
            _ => pieces.push(Piece::Word(word)),
        }
    };

    for ch in input.chars() {
        match ch {
            '"' if quoted => {
                quoted = false;
                pieces.push(Piece::Phrase(std::mem::take(&mut current)));
            }
            '"' => {
                flush(&mut current, &mut pieces);
                quoted = true;
            }
            c if c.is_whitespace() && !quoted => flush(&mut current, &mut pieces),
            c => current.push(c),
        }
    }
    if quoted {
        return Err(SearchError::Query(
            "the query has an opening quote with no closing one".to_owned(),
        ));
    }
    flush(&mut current, &mut pieces);
    Ok(pieces)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn terms(input: &str) -> Vec<String> {
        parse(input, "en")
            .expect("parses")
            .terms
            .into_iter()
            .map(|t| t.text)
            .collect()
    }

    #[test]
    fn words_are_tokenized_the_way_the_index_was() {
        assert_eq!(terms("Managing Connections"), ["manag", "connect"]);
    }

    #[test]
    fn only_the_last_word_matches_by_prefix() {
        let query = parse("rate lim", "en").expect("parses");
        assert!(!query.terms[0].prefix);
        assert!(query.terms[1].prefix);
    }

    #[test]
    fn a_stemmed_term_keeps_the_unstemmed_form_for_the_code_field() {
        let query = parse("getUserById", "en").expect("parses");
        assert!(
            query.terms[0].forms().any(|form| form == "getuserbyid"),
            "{:?}",
            query.terms[0]
        );
    }

    #[test]
    fn short_terms_are_not_fuzzy() {
        let query = parse("api limits", "en").expect("parses");
        assert!(!query.terms[0].fuzzy, "three letters is too short to guess");
        assert!(query.terms[1].fuzzy);
    }

    #[test]
    fn fuzziness_is_judged_on_what_was_typed_not_on_the_stem() {
        // `limts` stems to four characters, but the reader typed five, so the
        // typo is still worth one edit of tolerance (SRC-04).
        let query = parse("limts", "en").expect("parses");
        assert!(query.terms[0].text.chars().count() < FUZZY_MIN_LENGTH);
        assert!(query.terms[0].fuzzy);
    }

    #[test]
    fn a_quoted_run_is_a_phrase_and_still_contributes_terms() {
        let query = parse("\"rate limit\"", "en").expect("parses");
        assert_eq!(query.phrases.len(), 1);
        assert_eq!(query.phrases[0].len(), 2);
        assert_eq!(query.terms.len(), 2);
        assert!(!query.terms[0].prefix, "a closed quote is not a prefix");
    }

    #[test]
    fn a_one_word_quote_is_not_a_phrase() {
        let query = parse("\"limit\"", "en").expect("parses");
        assert!(query.phrases.is_empty());
        assert_eq!(query.terms.len(), 1);
    }

    #[test]
    fn an_unbalanced_quote_is_a_diagnostic() {
        let error = parse("\"rate limit", "en").expect_err("must not parse");
        assert_eq!(error.diagnostic().code.as_str(), "E1004");
    }

    #[test]
    fn a_facet_filter_leaves_the_query_terms_alone() {
        let query = parse("version:v2 limits", "en").expect("parses");
        assert_eq!(query.filters.version.as_deref(), Some("v2"));
        assert_eq!(query.terms.len(), 1);
    }

    #[test]
    fn every_facet_name_parses() {
        let query = parse("tab:docs version:v2 locale:de type:endpoint", "en").expect("parses");
        assert_eq!(query.filters.tab.as_deref(), Some("docs"));
        assert_eq!(query.filters.version.as_deref(), Some("v2"));
        assert_eq!(query.filters.locale.as_deref(), Some("de"));
        assert_eq!(query.filters.kind, Some(DocKind::Endpoint));
    }

    #[test]
    fn a_colon_in_ordinary_text_is_ordinary_text() {
        assert_eq!(terms("note: read this"), ["note", "read", "this"]);
    }

    #[test]
    fn an_unknown_content_type_is_a_diagnostic() {
        let error = parse("type:widget", "en").expect_err("must not parse");
        assert_eq!(error.diagnostic().code.as_str(), "E1004");
        assert!(error.to_string().contains("endpoint"), "{error}");
    }

    #[test]
    fn an_unknown_filter_name_names_the_ones_that_exist() {
        let mut filters = Filters::default();
        let error = filters.set("colour", "red").expect_err("must not set");
        assert!(error.to_string().contains("version"), "{error}");
    }

    #[test]
    fn a_repeated_term_is_indexed_once() {
        let query = parse("\"rate limit\" limit", "en").expect("parses");
        assert_eq!(query.terms.len(), 2);
        assert!(
            query.terms.iter().any(|t| t.prefix),
            "the trailing repeat still widens it to a prefix"
        );
    }

    #[test]
    fn an_empty_query_has_no_terms() {
        assert!(parse("   ", "en").expect("parses").is_empty());
        assert!(parse("", "en").expect("parses").is_empty());
    }

    #[test]
    fn the_raw_query_is_kept_for_analytics() {
        assert_eq!(
            parse("Rate Limits", "en").expect("parses").raw,
            "Rate Limits"
        );
    }

    #[test]
    fn a_reader_with_no_groups_sees_only_ungated_documents() {
        let anonymous = ReaderScope::default();
        assert!(anonymous.admits(&[], &[]));
        assert!(!anonymous.admits(&["beta".to_owned()], &[]));
    }

    #[test]
    fn a_reader_in_the_group_sees_the_document() {
        let reader = ReaderScope {
            groups: vec!["beta".to_owned()],
            region: Some("eu".to_owned()),
        };
        assert!(reader.admits(&["beta".to_owned()], &["eu".to_owned()]));
        assert!(!reader.admits(&["beta".to_owned()], &["us".to_owned()]));
        assert!(!reader.admits(&["alpha".to_owned()], &[]));
    }
}
