//! The code fence info string (CM-37, PRD §9.4).
//!
//! `lang [attributes]`, where an attribute is `key`, `key=value`, a quoted
//! value, or `{1,3-5}` for highlight ranges. An attribute nobody recognizes is
//! `W0302` and is kept, because the info string also has to survive a
//! round-trip through the formatter.

use std::ops::Range;

use liyasa_core::diagnostics::code;
use liyasa_core::document::FenceInfo;
use liyasa_core::{Diagnostic, SourceId, Span};

/// The attributes §9.4 defines. An unknown one is a warning, not an error.
pub const RECOGNIZED: &[&str] = &[
    "copy",
    "diff",
    "expandable",
    "filename",
    "focus",
    "icon",
    "lines",
    "maxLines",
    "prompt",
    "start",
    "template",
    "title",
    "verify",
    "wrap",
];

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Parsed {
    pub info: FenceInfo,
    /// Attribute names that are not in [`RECOGNIZED`], with where they sat.
    pub unknown: Vec<(String, Range<u32>)>,
}

impl Parsed {
    pub fn diagnostics(&self, source: SourceId, base: u32) -> Vec<Diagnostic> {
        self.unknown
            .iter()
            .map(|(name, at)| {
                Diagnostic::new(
                    code::W0302,
                    format!("unknown code fence attribute `{name}`"),
                )
                .at(Span::new(source, base + at.start, base + at.end))
                .help(format!(
                    "recognized attributes are {}",
                    RECOGNIZED.join(", ")
                ))
            })
            .collect()
    }
}

pub fn parse(info: &str) -> Parsed {
    let mut out = Parsed::default();
    let lead = info.len() - info.trim_start().len();
    let rest = &info[lead..];
    let lang_len = rest
        .find(|c: char| c.is_whitespace() || c == '{')
        .unwrap_or(rest.len());
    if lang_len > 0 {
        out.info.lang = Some(rest[..lang_len].to_owned());
    }

    let mut at = lead + lang_len;
    while at < info.len() {
        let rest = &info[at..];
        let skipped = rest.len() - rest.trim_start().len();
        at += skipped;
        if at >= info.len() {
            break;
        }
        let start = at;
        let used = attribute(&info[at..], &mut out);
        at += used.max(1);
        if let Some((_, span)) = out.unknown.last_mut().filter(|(_, at)| at.is_empty()) {
            *span = start as u32..at as u32;
        }
    }
    out
}

/// One attribute, returning how many bytes it consumed.
fn attribute(rest: &str, out: &mut Parsed) -> usize {
    if rest.starts_with('{') {
        let Some(end) = rest.find('}') else {
            return rest.len();
        };
        out.info.attrs.highlight.extend(ranges(&rest[1..end]));
        return end + 1;
    }
    let key_len = rest
        .find(|c: char| c == '=' || c.is_whitespace())
        .unwrap_or(rest.len());
    let key = &rest[..key_len];
    if key.is_empty() {
        return 1;
    }
    if !RECOGNIZED.contains(&key) {
        // The span is filled in by the caller, which knows the base offset.
        out.unknown.push((key.to_owned(), 0..0));
    }
    let Some(after) = rest[key_len..].strip_prefix('=') else {
        out.info.attrs.flags.insert(key.to_owned());
        return key_len;
    };
    let (value, used) = value(after);
    out.info.attrs.kv.insert(key.to_owned(), value);
    key_len + 1 + used
}

fn value(text: &str) -> (String, usize) {
    if let Some(after) = text.strip_prefix('"') {
        return match after.find('"') {
            Some(end) => (after[..end].to_owned(), end + 2),
            None => (after.to_owned(), text.len()),
        };
    }
    if let Some(after) = text.strip_prefix('{') {
        return match after.find('}') {
            Some(end) => (after[..end].to_owned(), end + 2),
            None => (after.to_owned(), text.len()),
        };
    }
    let len = text.find(char::is_whitespace).unwrap_or(text.len());
    (text[..len].to_owned(), len)
}

/// `1,3-5` as inclusive 1-based line ranges.
fn ranges(text: &str) -> Vec<(u32, u32)> {
    text.split(',')
        .filter_map(|part| {
            let part = part.trim();
            match part.split_once('-') {
                Some((first, last)) => {
                    Some((first.trim().parse().ok()?, last.trim().parse().ok()?))
                }
                None => {
                    let only = part.parse().ok()?;
                    Some((only, only))
                }
            }
        })
        .collect()
}

#[cfg(test)]
mod tests;
