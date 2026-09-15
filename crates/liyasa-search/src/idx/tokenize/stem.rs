//! Snowball stemming for the seventeen languages SRC-02 names.
//!
//! tantivy's own stemmer filter is not used: the browser index has no tantivy
//! in it, and two Snowball implementations would be two sets of stems. Both
//! sides call this function with the same pinned `rust-stemmers` version, and
//! the parity corpus asserts stem equality per language (§12.2).

use std::borrow::Cow;

pub use rust_stemmers::Algorithm;
use rust_stemmers::Stemmer;

/// The primary language subtags SRC-02 names, in the order §12.2 lists them.
pub const STEMMED_LOCALES: &[&str] = &[
    "en", "de", "fr", "es", "pt", "it", "nl", "ru", "sv", "no", "da", "fi", "hu", "ro", "tr", "ar",
    "el",
];

/// The Snowball algorithm for a BCP 47 tag, by primary subtag: `pt-BR` and
/// `pt` stem alike, as they must for a query typed in either to match.
pub fn algorithm_for(locale: &str) -> Option<Algorithm> {
    let primary = locale
        .split(['-', '_'])
        .next()
        .unwrap_or(locale)
        .to_ascii_lowercase();
    Some(match primary.as_str() {
        "ar" => Algorithm::Arabic,
        "da" => Algorithm::Danish,
        "de" => Algorithm::German,
        "el" => Algorithm::Greek,
        "en" => Algorithm::English,
        "es" => Algorithm::Spanish,
        "fi" => Algorithm::Finnish,
        "fr" => Algorithm::French,
        "hu" => Algorithm::Hungarian,
        "it" => Algorithm::Italian,
        "nl" => Algorithm::Dutch,
        // `no` is the macrolanguage; documentation is written in one of its
        // two written standards, and Snowball has one algorithm for both.
        "no" | "nb" | "nn" => Algorithm::Norwegian,
        "pt" => Algorithm::Portuguese,
        "ro" => Algorithm::Romanian,
        "ru" => Algorithm::Russian,
        "sv" => Algorithm::Swedish,
        "ta" => Algorithm::Tamil,
        "tr" => Algorithm::Turkish,
        _ => return None,
    })
}

/// Stems an already-lowercased term. `None` returns it unchanged, which is how
/// the languages Snowball has no algorithm for are indexed.
///
/// Building the `Stemmer` is not free, so a caller with more than one term to
/// stem holds one across them; [`Tokenizer::tokenize`] does.
///
/// [`Tokenizer::tokenize`]: super::Tokenizer::tokenize
pub fn stem(lowercased: &str, algorithm: Option<Algorithm>) -> Cow<'_, str> {
    match algorithm {
        Some(algorithm) => Cow::Owned(Stemmer::create(algorithm).stem(lowercased).into_owned()),
        None => Cow::Borrowed(lowercased),
    }
}

/// The stemmer for an algorithm, built once and reused across a token stream.
pub fn stemmer(algorithm: Option<Algorithm>) -> Option<Stemmer> {
    algorithm.map(Stemmer::create)
}
