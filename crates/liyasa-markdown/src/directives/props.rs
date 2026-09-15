//! The directive prop grammar (PRD §7.5.1, CM-51).
//!
//! `{key="string" key=123 key=true key=[a,b] key={{ expr }} .class #id}`.
//!
//! The parser never refuses: comrak has already decided the line is a
//! directive, so a malformed prop list yields `E0312` and whatever props were
//! recoverable, not a rejection of the block. Offsets are relative to the text
//! handed in; [`diagnostics`] turns them into spans against a base.

use std::ops::Range;

use liyasa_core::diagnostics::code;
use liyasa_core::document::{PropValue, Props};
use liyasa_core::{Diagnostic, SourceId, Span};

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Parsed {
    pub props: Props,
    /// Where each prop was written, relative to the parsed text.
    pub spans: Vec<(String, Range<u32>)>,
    pub errors: Vec<Error>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    pub at: Range<u32>,
    pub message: String,
}

impl Parsed {
    pub fn span_of(&self, name: &str) -> Option<Range<u32>> {
        self.spans
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, at)| at.clone())
    }
}

/// `E0312` for each error, with offsets shifted onto `base`.
pub fn diagnostics(
    parsed: &Parsed,
    source: SourceId,
    base: u32,
) -> impl Iterator<Item = Diagnostic> + '_ {
    parsed.errors.iter().map(move |error| {
        Diagnostic::new(code::E0312, error.message.clone()).at(Span::new(
            source,
            base + error.at.start,
            base + error.at.end,
        ))
    })
}

/// Parses the tail of a directive: everything after the name.
pub fn parse(text: &str) -> Parsed {
    let mut out = Parsed::default();
    let open = text.find(|c: char| !c.is_whitespace());
    let Some(open) = open else {
        return out;
    };
    if !text[open..].starts_with('{') {
        out.errors.push(Error {
            at: offsets(open, text.trim_end().len()),
            message: "directive props must be written as `{key=value}`".to_owned(),
        });
        return out;
    }
    let end = text.trim_end().len();
    if !text[..end].ends_with('}') || end == open + 1 {
        out.errors.push(Error {
            at: offsets(open, end),
            message: "unclosed `{` in directive props".to_owned(),
        });
        return out;
    }
    scan(text, open + 1, end - 1, &mut out);
    out
}

fn scan(text: &str, from: usize, to: usize, out: &mut Parsed) {
    let mut at = from;
    while at < to {
        let rest = &text[at..to];
        let skipped = rest.len() - rest.trim_start().len();
        at += skipped;
        if at >= to {
            break;
        }
        let start = at;
        let rest = &text[at..to];
        let entry = match rest.as_bytes()[0] {
            b'.' => shorthand(rest, "class"),
            b'#' => shorthand(rest, "id"),
            _ => pair(rest),
        };
        match entry {
            Ok((name, value, used)) => {
                at += used;
                insert(out, name, value, offsets(start, at));
            }
            Err((message, used)) => {
                // Always consume something, or a malformed prop list loops.
                at += used.max(1);
                out.errors.push(Error {
                    at: offsets(start, at),
                    message,
                });
            }
        }
    }
}

/// `class` and `id` accumulate, because `{.a .b}` means both classes.
fn insert(out: &mut Parsed, name: String, value: PropValue, at: Range<u32>) {
    let joined = match (out.props.0.get(&name), &value) {
        (Some(PropValue::Str(before)), PropValue::Str(after)) if name == "class" => {
            Some(PropValue::Str(format!("{before} {after}")))
        }
        _ => None,
    };
    out.props.0.insert(name.clone(), joined.unwrap_or(value));
    out.spans.retain(|(key, _)| *key != name);
    out.spans.push((name, at));
}

type Entry = (String, PropValue, usize);
type Failure = (String, usize);

fn shorthand(rest: &str, name: &str) -> Result<Entry, Failure> {
    let len = token_len(&rest[1..]);
    if len == 0 {
        return Err((format!("`{}` needs a name after it", &rest[..1]), 1));
    }
    Ok((
        name.to_owned(),
        PropValue::Str(rest[1..1 + len].to_owned()),
        1 + len,
    ))
}

fn pair(rest: &str) -> Result<Entry, Failure> {
    let key_len = rest
        .find(|c: char| c == '=' || c.is_whitespace())
        .unwrap_or(rest.len());
    let key = &rest[..key_len];
    if key.is_empty() {
        return Err((format!("`{}` is not a prop name", &rest[..1]), 1));
    }
    let gap = &rest[key_len..];
    let before = gap.len() - gap.trim_start().len();
    if !gap[before..].starts_with('=') {
        // A prop with no value is the flag it looks like, as it is in the tag
        // form and in HTML. See `plan/rfcs/0027-bare-flag-props.md`.
        return Ok((key.to_owned(), PropValue::Bool(true), key_len));
    }
    let after = &gap[before + 1..];
    let lead = after.len() - after.trim_start().len();
    let (value, used) = value(&after[lead..]).map_err(|message| (message, rest.len()))?;
    Ok((key.to_owned(), value, key_len + before + 1 + lead + used))
}

fn value(text: &str) -> Result<(PropValue, usize), String> {
    if let Some(after) = text.strip_prefix('"') {
        let end = after
            .find('"')
            .ok_or_else(|| "unterminated string in directive props".to_owned())?;
        return Ok((PropValue::Str(after[..end].to_owned()), end + 2));
    }
    if let Some(after) = text.strip_prefix("{{") {
        let end = after
            .find("}}")
            .ok_or_else(|| "unterminated `{{` expression in directive props".to_owned())?;
        return Ok((PropValue::Expr(after[..end].trim().to_owned()), end + 4));
    }
    if let Some(after) = text.strip_prefix('[') {
        let end =
            list_end(after).ok_or_else(|| "unterminated `[` list in directive props".to_owned())?;
        return Ok((PropValue::List(items(&after[..end])), end + 2));
    }
    let len = text
        .find(|c: char| c.is_whitespace() || c == '}')
        .unwrap_or(text.len());
    Ok((scalar(&text[..len]), len))
}

/// A bare token as a typed value: the tag form shares this with the brace form,
/// so `<Card columns=2>` and `:::card{columns=2}` agree.
pub fn scalar_value(token: &str) -> PropValue {
    scalar(token)
}

fn scalar(token: &str) -> PropValue {
    match token {
        "true" => PropValue::Bool(true),
        "false" => PropValue::Bool(false),
        // `inf` and `nan` parse as f64 and have no JSON form, so a prop is a
        // number only when it is written like one.
        _ if token.starts_with(|c: char| c.is_ascii_digit() || c == '-' || c == '+') => token
            .parse::<f64>()
            .ok()
            .filter(|n| n.is_finite())
            .map_or_else(|| PropValue::Str(token.to_owned()), PropValue::Num),
        _ => PropValue::Str(token.to_owned()),
    }
}

/// The offset of the `]` that closes a list, skipping quoted commas.
fn list_end(text: &str) -> Option<usize> {
    let mut quoted = false;
    for (at, ch) in text.char_indices() {
        match ch {
            '"' => quoted = !quoted,
            ']' if !quoted => return Some(at),
            _ => {}
        }
    }
    None
}

fn items(body: &str) -> Vec<PropValue> {
    let mut out = Vec::new();
    let mut quoted = false;
    let mut start = 0;
    for (at, ch) in body.char_indices() {
        match ch {
            '"' => quoted = !quoted,
            ',' if !quoted => {
                out.push(item(&body[start..at]));
                start = at + 1;
            }
            _ => {}
        }
    }
    let last = &body[start..];
    if !out.is_empty() || !last.trim().is_empty() {
        out.push(item(last));
    }
    out
}

fn item(text: &str) -> PropValue {
    let text = text.trim();
    match text.strip_prefix('"').and_then(|t| t.strip_suffix('"')) {
        Some(quoted) => PropValue::Str(quoted.to_owned()),
        None => scalar(text),
    }
}

fn token_len(text: &str) -> usize {
    text.find(|c: char| !(c.is_alphanumeric() || c == '-' || c == '_'))
        .unwrap_or(text.len())
}

fn offsets(start: usize, end: usize) -> Range<u32> {
    start as u32..end as u32
}

#[cfg(test)]
mod tests;
