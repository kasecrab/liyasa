//! The info string of a container directive (`plan/rfcs/0003-parser-spike.md`).
//!
//! comrak 0.55 owns container segmentation and hands the info string over
//! verbatim, so nothing an author writes in a prop value can terminate the
//! construct that carries it. What is left is naming the component and reading
//! its props.

use liyasa_core::diagnostics::code;
use liyasa_core::document::Props;
use liyasa_core::{Diagnostic, SourceId, Span};

use super::props;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Info {
    pub name: String,
    pub props: Props,
    /// Where the name was written, relative to the info string.
    pub name_at: std::ops::Range<u32>,
    parsed: props::Parsed,
}

impl Info {
    /// Where a prop was written, relative to the info string.
    pub fn span_of(&self, name: &str) -> Option<std::ops::Range<u32>> {
        self.parsed.span_of(name)
    }

    pub fn is_empty(&self) -> bool {
        self.name.is_empty() && self.props.is_empty()
    }

    /// Every diagnostic the info string itself raises, against a base offset in
    /// whichever coordinate space the caller is working in.
    pub fn diagnostics(&self, source: SourceId, base: u32) -> Vec<Diagnostic> {
        let mut out: Vec<_> = props::diagnostics(&self.parsed, source, base).collect();
        if self.name.is_empty() && !self.props.is_empty() {
            out.push(
                Diagnostic::new(code::E0312, "directive has props but no name").at(Span::new(
                    source,
                    base,
                    base + self.name_at.end,
                )),
            );
        }
        out
    }
}

/// A directive name is what could also be an HTML tag or a kebab-case
/// component: letters, digits, `-`, and `_`.
pub fn parse(info: &str) -> Info {
    let lead = info.len() - info.trim_start().len();
    let rest = &info[lead..];
    let name_len = rest
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '_'))
        .unwrap_or(rest.len());
    let parsed = props::parse(&rest[name_len..]);
    let shift = (lead + name_len) as u32;
    Info {
        name: rest[..name_len].to_owned(),
        props: parsed.props.clone(),
        name_at: lead as u32..lead as u32 + name_len as u32,
        parsed: props::Parsed {
            spans: parsed
                .spans
                .iter()
                .map(|(key, at)| (key.clone(), at.start + shift..at.end + shift))
                .collect(),
            errors: parsed
                .errors
                .iter()
                .map(|error| props::Error {
                    at: error.at.start + shift..error.at.end + shift,
                    message: error.message.clone(),
                })
                .collect(),
            props: parsed.props,
        },
    }
}

#[cfg(test)]
mod tests;
