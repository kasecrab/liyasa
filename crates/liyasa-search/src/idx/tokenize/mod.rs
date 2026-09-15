//! The tokenizers both indexes share (PRD §12.2, SRC-02).
//!
//! Parity is the reason this is one module rather than two implementations:
//! the writer, the server's tantivy analyzer, and the browser's query-time
//! tokenizer all call these functions, so a query term arrives at the term
//! dictionary in exactly the form the writer put there.

mod cjk;
mod code;
mod stem;

pub use cjk::{Script, cjk_bigram, script_of};
pub use code::code;
pub use stem::{Algorithm, STEMMED_LOCALES, algorithm_for, stem};

/// One indexed term with everything the index needs: the term itself, its
/// ordinal for phrase matching, and the byte range it came from for snippet
/// highlighting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub text: String,
    pub position: u32,
    pub start: u32,
    pub end: u32,
}

/// The analyzer for prose fields: `title`, `section`, `breadcrumb`, `body`,
/// and `keywords`. The `code` field uses [`code`] instead.
#[derive(Debug, Clone)]
pub struct Tokenizer {
    algorithm: Option<Algorithm>,
}

impl Tokenizer {
    /// Picks the Snowball algorithm for a BCP 47 tag. A tag with no algorithm
    /// indexes unstemmed, which the build reports once as `W1001`.
    pub fn for_locale(locale: &str) -> Self {
        Self {
            algorithm: algorithm_for(locale),
        }
    }

    pub fn unstemmed() -> Self {
        Self { algorithm: None }
    }

    pub fn algorithm(&self) -> Option<Algorithm> {
        self.algorithm
    }

    /// Splits `text` into terms: CJK runs become bigrams whatever the locale
    /// says, every other run of alphanumerics is lowercased and stemmed.
    pub fn tokenize(&self, text: &str) -> Vec<Token> {
        let mut out = Vec::new();
        let mut position = 0u32;
        let mut word: Option<(usize, usize)> = None;

        let flush =
            |word: &mut Option<(usize, usize)>, out: &mut Vec<Token>, position: &mut u32| {
                if let Some((start, end)) = word.take() {
                    let lowered = text[start..end].to_lowercase();
                    out.push(Token {
                        text: stem(&lowered, self.algorithm).into_owned(),
                        position: *position,
                        start: start as u32,
                        end: end as u32,
                    });
                    *position += 1;
                }
            };

        let mut chars = text.char_indices().peekable();
        while let Some((at, ch)) = chars.next() {
            match script_of(ch) {
                Some(script) => {
                    flush(&mut word, &mut out, &mut position);
                    let mut run_end = at + ch.len_utf8();
                    while let Some(&(next_at, next)) = chars.peek() {
                        if script_of(next) != Some(script) {
                            break;
                        }
                        run_end = next_at + next.len_utf8();
                        chars.next();
                    }
                    for token in cjk_bigram(&text[at..run_end], at as u32, &mut position) {
                        out.push(token);
                    }
                }
                None if ch.is_alphanumeric() => {
                    let end = at + ch.len_utf8();
                    word = Some(match word {
                        Some((start, _)) => (start, end),
                        None => (at, end),
                    });
                }
                None => flush(&mut word, &mut out, &mut position),
            }
        }
        flush(&mut word, &mut out, &mut position);
        out
    }
}
