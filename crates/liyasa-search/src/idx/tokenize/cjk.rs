//! Dictionary-free CJK segmentation (SRC-02).
//!
//! Overlapping bigrams within one script, the arrangement Lucene's
//! `CJKBigramFilter` settled on: no dictionary to ship, no per-language build,
//! and recall close enough to a segmenter that the difference is not worth
//! 60 MB of dictionary in the base binary. Dictionary segmentation is the
//! `cjk-dict` feature, which is a seam until `liyasa add dictionary` can
//! fetch one: plan/rfcs/0701-cjk-dictionaries.md.
// TODO(rfc-0701): `Segmenter` gains a lindera implementation behind `cjk-dict`.

use super::Token;

/// The script classes bigrams are formed within. A bigram never spans two of
/// them, so `検索エンジン` is `検索` plus the katakana run's three bigrams
/// rather than a `索エ` that means nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Script {
    Han,
    Hiragana,
    Katakana,
    Hangul,
}

/// `Some` for a character that is segmented by bigram rather than by
/// whitespace.
pub fn script_of(ch: char) -> Option<Script> {
    match ch as u32 {
        0x3040..=0x309F => Some(Script::Hiragana),
        0x30A0..=0x30FF | 0x31F0..=0x31FF | 0xFF66..=0xFF9D => Some(Script::Katakana),
        0x1100..=0x11FF | 0x3130..=0x318F | 0xA960..=0xA97F | 0xAC00..=0xD7FF => {
            Some(Script::Hangul)
        }
        0x2E80..=0x2EFF
        | 0x3400..=0x4DBF
        | 0x4E00..=0x9FFF
        | 0xF900..=0xFAFF
        | 0x20000..=0x2FA1F => Some(Script::Han),
        _ => None,
    }
}

/// Overlapping bigrams over one same-script run. A run of one character is its
/// own term, so a single-character query still matches.
pub fn cjk_bigram(run: &str, offset: u32, position: &mut u32) -> Vec<Token> {
    let chars: Vec<(usize, char)> = run.char_indices().collect();
    let mut out = Vec::with_capacity(chars.len());
    if chars.len() == 1 {
        let (at, ch) = chars[0];
        out.push(Token {
            text: ch.to_string(),
            position: *position,
            start: offset + at as u32,
            end: offset + (at + ch.len_utf8()) as u32,
        });
        *position += 1;
        return out;
    }
    for pair in chars.windows(2) {
        let (start, first) = pair[0];
        let (at, second) = pair[1];
        let end = at + second.len_utf8();
        out.push(Token {
            text: format!("{first}{second}"),
            position: *position,
            start: offset + start as u32,
            end: offset + end as u32,
        });
        *position += 1;
    }
    out
}
