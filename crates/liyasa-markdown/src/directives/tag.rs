//! The tag form `<Name …>` (§7.5.1 item 4, CM-53).
//!
//! It exists so a site migrating from MDX can be built before it is rewritten.
//! comrak reports these as `HtmlBlock` and `HtmlInline`, because they are
//! syntactically HTML; the only thing that separates a component from an
//! element is the capital letter, which HTML tag names never carry.
//!
//! Attributes use HTML spelling with the directive value grammar underneath,
//! so `<Card columns=2>` and `:::card{columns=2}` produce the same typed props.

use liyasa_core::document::{PropValue, Props};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Form {
    Open,
    Close,
    SelfClosing,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Tag {
    /// As written, so the registry can resolve it through a component's
    /// aliases and report the canonical kebab-case name.
    pub name: String,
    pub props: Props,
    pub form: Form,
}

/// A component tag, or `None` for anything else — including every lowercase
/// tag, which stays raw HTML and goes to the sanitizer.
pub fn parse(html: &str) -> Option<Tag> {
    let text = html.trim();
    let body = text.strip_prefix('<')?.strip_suffix('>')?;
    let (body, form) = match body.strip_prefix('/') {
        Some(rest) => (rest, Form::Close),
        None => match body.strip_suffix('/') {
            Some(rest) => (rest, Form::SelfClosing),
            None => (body, Form::Open),
        },
    };
    let body = body.trim();
    let name_len = body
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '_'))
        .unwrap_or(body.len());
    let name = &body[..name_len];
    if !name.starts_with(|c: char| c.is_ascii_uppercase()) {
        return None;
    }
    if form == Form::Close && !body[name_len..].trim().is_empty() {
        return None;
    }
    Some(Tag {
        props: attributes(&body[name_len..]),
        name: name.to_owned(),
        form,
    })
}

/// The kebab-case directive name a tag stands for, for the formatter's
/// `<Card title="x">` to `:::card{title="x"}` conversion. The registry has the
/// last word; this is what to look up and what to fall back to.
pub fn directive_name(tag: &str) -> String {
    let mut out = String::with_capacity(tag.len() + 4);
    for (at, ch) in tag.char_indices() {
        if ch.is_ascii_uppercase() && at > 0 && !out.ends_with('-') {
            out.push('-');
        }
        out.extend(ch.to_lowercase());
    }
    out
}

fn attributes(text: &str) -> Props {
    let mut out = Props::default();
    let mut rest = text.trim();
    while !rest.is_empty() {
        let name_len = rest
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == ':'))
            .unwrap_or(rest.len());
        if name_len == 0 {
            // Not an attribute name; skip a character so the scan terminates.
            rest = rest[rest.chars().next().map_or(1, char::len_utf8)..].trim_start();
            continue;
        }
        let name = rest[..name_len].to_owned();
        let after = rest[name_len..].trim_start();
        let Some(after) = after.strip_prefix('=') else {
            // A bare attribute is a flag, as it is in HTML.
            out.0.insert(name, PropValue::Bool(true));
            rest = after;
            continue;
        };
        let (value, used) = value(after.trim_start());
        out.0.insert(name, value);
        rest = after.trim_start()[used..].trim_start();
    }
    out
}

fn value(text: &str) -> (PropValue, usize) {
    for quote in ['"', '\''] {
        if let Some(after) = text.strip_prefix(quote) {
            return match after.find(quote) {
                Some(end) => (PropValue::Str(after[..end].to_owned()), end + 2),
                None => (PropValue::Str(after.to_owned()), text.len()),
            };
        }
    }
    if let Some(after) = text.strip_prefix("{{") {
        return match after.find("}}") {
            Some(end) => (PropValue::Expr(after[..end].trim().to_owned()), end + 4),
            None => (PropValue::Expr(after.trim().to_owned()), text.len()),
        };
    }
    let len = text.find(char::is_whitespace).unwrap_or(text.len());
    (super::props::scalar_value(&text[..len]), len)
}

#[cfg(test)]
mod tests;
