//! The code tokenizer (SRC-02): `getUserById` matches `user id`.
//!
//! Identifiers are split on case, on the letter/digit boundary, and on the
//! punctuation that joins words in `snake_case`, `kebab-case`, and dotted
//! config keys. The whole identifier is kept alongside its parts, at the first
//! part's position, so searching for `getUserById` still ranks the exact
//! symbol first. Code is never stemmed: `running` is a method name, not a verb.

use super::Token;

/// Characters that hold an identifier together rather than ending it.
fn joins(ch: char) -> bool {
    matches!(ch, '_' | '-' | '.')
}

pub fn code(text: &str) -> Vec<Token> {
    let mut out = Vec::new();
    let mut position = 0u32;
    let mut run: Option<(usize, usize)> = None;

    let emit = |run: &mut Option<(usize, usize)>, out: &mut Vec<Token>, position: &mut u32| {
        let Some((start, end)) = run.take() else {
            return;
        };
        let whole = &text[start..end];
        let words: Vec<(usize, &str)> = split_words(whole);
        let split: Vec<(usize, Vec<(usize, &str)>)> = words
            .iter()
            .map(|&(at, word)| (at, split_case(word)))
            .collect();
        let parts: usize = split.iter().map(|(_, parts)| parts.len()).sum();
        if parts == 0 {
            return;
        }
        if parts > 1 {
            out.push(Token {
                text: whole.to_lowercase(),
                position: *position,
                start: start as u32,
                end: end as u32,
            });
        }
        for (word_at, word_parts) in split {
            // `client.getUserById` must also be findable as `getUserById`, so
            // a compound word inside a dotted or underscored run keeps a token
            // of its own.
            if words.len() > 1 && word_parts.len() > 1 {
                let word = words
                    .iter()
                    .find(|&&(at, _)| at == word_at)
                    .map_or("", |&(_, word)| word);
                out.push(Token {
                    text: word.to_lowercase(),
                    position: *position,
                    start: (start + word_at) as u32,
                    end: (start + word_at + word.len()) as u32,
                });
            }
            for (at, part) in word_parts {
                out.push(Token {
                    text: part.to_lowercase(),
                    position: *position,
                    start: (start + word_at + at) as u32,
                    end: (start + word_at + at + part.len()) as u32,
                });
                *position += 1;
            }
        }
    };

    for (at, ch) in text.char_indices() {
        let end = at + ch.len_utf8();
        if ch.is_alphanumeric() || joins(ch) {
            run = Some(match run {
                Some((start, _)) => (start, end),
                None => (at, end),
            });
        } else {
            emit(&mut run, &mut out, &mut position);
        }
    }
    emit(&mut run, &mut out, &mut position);
    out
}

/// One run into the words the join characters separate, each with its byte
/// offset within the run.
fn split_words(run: &str) -> Vec<(usize, &str)> {
    let mut out = Vec::new();
    let mut start: Option<usize> = None;
    for (at, ch) in run.char_indices() {
        if joins(ch) {
            if let Some(from) = start.take() {
                out.push((from, &run[from..at]));
            }
        } else {
            start.get_or_insert(at);
        }
    }
    if let Some(from) = start {
        out.push((from, &run[from..]));
    }
    out
}

/// One word into its case and digit parts, each with its byte offset within
/// the word.
fn split_case(word: &str) -> Vec<(usize, &str)> {
    let mut parts = Vec::new();
    let chars: Vec<(usize, char)> = word.char_indices().collect();
    let mut start: Option<usize> = None;

    for (n, &(at, ch)) in chars.iter().enumerate() {
        let previous = n.checked_sub(1).map(|p| chars[p].1);
        let next = chars.get(n + 1).map(|&(_, c)| c);
        let boundary = match previous {
            None => false,
            // `parseJSON`, `utf8Decode`, `id2Name`: the class changed.
            Some(p) if !p.is_uppercase() && ch.is_uppercase() => true,
            Some(p) if p.is_numeric() != ch.is_numeric() => true,
            // `HTTPServer`: the last capital of a run starts the next word.
            Some(p) if p.is_uppercase() && ch.is_uppercase() => {
                next.is_some_and(|c| c.is_lowercase())
            }
            Some(_) => false,
        };
        if boundary && let Some(from) = start.take() {
            parts.push((from, &word[from..at]));
        }
        start.get_or_insert(at);
    }
    if let Some(from) = start {
        parts.push((from, &word[from..]));
    }
    parts
}
