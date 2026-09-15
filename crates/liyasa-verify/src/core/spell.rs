//! Spelling, with a project dictionary and code-aware tokenization (VER-62).
//!
//! The tokenizer is the whole feature. A documentation spell checker that does
//! not understand code reports `serde_json`, `kubectl`, `x86_64`, and every
//! URL, and an author turns it off within a day. So a token that looks like
//! code is not a word: identifiers in any case convention, anything with a
//! digit or an underscore, file names, versions, hex, URLs, and the contents
//! of inline code are skipped before the dictionary is consulted.

use std::collections::BTreeSet;

use liyasa_core::diagnostics::{Diagnostic, code};

/// A word the checker found and could not place.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Misspelling {
    pub word: String,
    /// Byte offset in the text that was checked.
    pub at: usize,
}

impl Misspelling {
    pub fn diagnostic(&self) -> Diagnostic {
        Diagnostic::new(
            code::W0632,
            format!("`{}` is not in the project dictionary", self.word),
        )
        .help("add it to the dictionary, or write it in backticks if it is code")
    }
}

#[derive(Debug, Clone, Default)]
pub struct Dictionary {
    words: BTreeSet<String>,
}

impl Dictionary {
    pub fn new() -> Self {
        Self::default()
    }

    /// One word per line; blank lines and `#` comments are skipped. This is
    /// the format Vale's vocabularies and every `.dic` sidecar use.
    pub fn from_lines(text: &str) -> Self {
        let mut out = Self::new();
        out.extend(text.lines());
        out
    }

    pub fn extend<'a>(&mut self, words: impl IntoIterator<Item = &'a str>) {
        for word in words {
            let word = word.split('#').next().unwrap_or_default().trim();
            if !word.is_empty() {
                self.words.insert(word.to_ascii_lowercase());
            }
        }
    }

    pub fn contains(&self, word: &str) -> bool {
        self.words.contains(&word.to_ascii_lowercase())
    }

    pub fn len(&self) -> usize {
        self.words.len()
    }

    pub fn is_empty(&self) -> bool {
        self.words.is_empty()
    }
}

pub struct SpellChecker {
    dictionary: Dictionary,
    /// Words to pass whatever the dictionary says, from a rule's `ignore`.
    ignore: BTreeSet<String>,
}

impl SpellChecker {
    pub fn new(dictionary: Dictionary) -> Self {
        Self {
            dictionary,
            ignore: BTreeSet::new(),
        }
    }

    #[must_use]
    pub fn ignoring<'a>(mut self, words: impl IntoIterator<Item = &'a str>) -> Self {
        self.ignore
            .extend(words.into_iter().map(str::to_ascii_lowercase));
        self
    }

    pub fn dictionary(&self) -> &Dictionary {
        &self.dictionary
    }

    pub fn check(&self, text: &str) -> Vec<Misspelling> {
        words(text)
            .into_iter()
            .filter(|(word, _)| !self.knows(word))
            .map(|(word, at)| Misspelling { word, at })
            .collect()
    }

    fn knows(&self, word: &str) -> bool {
        let lower = word.to_ascii_lowercase();
        if self.ignore.contains(&lower) || self.dictionary.contains(&lower) {
            return true;
        }
        // A possessive or plural of a known word is known.
        for suffix in ["'s", "\u{2019}s", "s"] {
            if let Some(stem) = lower.strip_suffix(suffix)
                && !stem.is_empty()
                && self.dictionary.contains(stem)
            {
                return true;
            }
        }
        false
    }
}

/// Every word in `text` that is worth spelling, with its byte offset.
pub fn words(text: &str) -> Vec<(String, usize)> {
    let mut out = Vec::new();
    for (token, at) in tokens(text) {
        if is_code(token) {
            continue;
        }
        // A hyphenated compound is spelled a part at a time, because
        // dictionaries hold the parts.
        let mut offset = 0usize;
        for part in token.split(['-', '\u{2010}', '\u{2013}']) {
            let start = at + offset;
            offset += part.len() + 1;
            let part = part.trim_matches(|c: char| !c.is_alphabetic() && c != '\'');
            if part.chars().count() > 1 && part.chars().all(|c| c.is_alphabetic() || c == '\'') {
                out.push((part.to_owned(), start));
            }
        }
    }
    out
}

/// Runs of non-space, so a token keeps the punctuation that says what it is:
/// a trailing `()` makes `format()` code, and `.md` makes `install.md` a file.
fn tokens(text: &str) -> Vec<(&str, usize)> {
    let mut out = Vec::new();
    let mut start = None;
    for (at, c) in text.char_indices() {
        if c.is_whitespace() {
            if let Some(from) = start.take() {
                out.push((&text[from..at], from));
            }
        } else if start.is_none() {
            start = Some(at);
        }
    }
    if let Some(from) = start {
        out.push((&text[from..], from));
    }
    out
}

/// Whether a token is code rather than prose.
fn is_code(token: &str) -> bool {
    // The call parentheses are read before trimming, or `format()` trims down
    // to a perfectly ordinary word.
    if token
        .trim_end_matches(['`', '.', ',', ';', ':', ')'])
        .ends_with("()")
        || token.contains("()")
    {
        return true;
    }
    let bare = token.trim_matches(|c: char| c.is_ascii_punctuation() && c != '_' && c != '/');
    if bare.is_empty() {
        return true;
    }
    if bare.contains("://") || bare.starts_with("www.") || bare.contains('@') {
        return true; // a URL or an address
    }
    if bare.contains('/') || bare.contains('\\') {
        return true; // a path
    }
    if bare.contains('_') || bare.contains("::") {
        return true; // snake_case, SCREAMING_CASE, a Rust path
    }
    if bare.chars().any(|c| c.is_ascii_digit()) {
        return true; // a version, a hex digest, x86_64, HTTP/2
    }
    if has_inner_dot(bare) {
        return true; // a file name, a dotted key, a domain
    }
    if is_camel_case(bare) {
        return true; // camelCase and PascalCase with an inner capital
    }
    false
}

/// A dot with a letter on both sides: `install.md`, `facts.pricing`. A
/// sentence-ending dot has whitespace after it, so it never reaches here.
fn has_inner_dot(token: &str) -> bool {
    token
        .char_indices()
        .filter(|(_, c)| *c == '.')
        .any(|(at, _)| at > 0 && at + 1 < token.len())
}

/// A capital after a lowercase letter: `camelCase`, `JsonValue`, `liyasaBuild`.
/// An ordinary capitalized word has none, and `HTTP` has no lowercase before
/// its capitals.
fn is_camel_case(token: &str) -> bool {
    let mut seen_lower = false;
    for c in token.chars() {
        if c.is_lowercase() {
            seen_lower = true;
        } else if c.is_uppercase() && seen_lower {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests;
