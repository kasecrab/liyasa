//! A tag scanner for the raw HTML that reaches the sanitizer.
//!
//! comrak hands raw HTML over as text, so the sanitizer has to find the tags in
//! it. This is not a parser: it finds tag boundaries, names, and attributes, and
//! is deliberately generous about what counts as a tag, because a tag the
//! scanner misses is a tag the sanitizer does not filter.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Token<'a> {
    Text(&'a str),
    /// `<!-- … -->`, a doctype, or a processing instruction.
    Bogus(&'a str),
    Tag(Tag<'a>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tag<'a> {
    pub name: String,
    pub closing: bool,
    pub self_closing: bool,
    pub attributes: Vec<(String, Option<&'a str>)>,
    pub source: &'a str,
}

/// Splits raw HTML into text, comments, and tags.
pub fn tokenize(html: &str) -> Vec<Token<'_>> {
    let mut out = Vec::new();
    let mut at = 0;
    let mut text = 0;
    while let Some(found) = html[at..].find('<') {
        let start = at + found;
        let Some((token, end)) = token_at(html, start) else {
            at = start + 1;
            continue;
        };
        if text < start {
            out.push(Token::Text(&html[text..start]));
        }
        out.push(token);
        at = end;
        text = end;
    }
    if text < html.len() {
        out.push(Token::Text(&html[text..]));
    }
    out
}

fn token_at(html: &str, start: usize) -> Option<(Token<'_>, usize)> {
    let rest = &html[start..];
    if rest.starts_with("<!--") {
        let end = rest.find("-->").map_or(html.len(), |at| start + at + 3);
        return Some((Token::Bogus(&html[start..end]), end));
    }
    if rest.starts_with("<!") || rest.starts_with("<?") {
        let end = rest.find('>').map_or(html.len(), |at| start + at + 1);
        return Some((Token::Bogus(&html[start..end]), end));
    }

    let body = rest.strip_prefix('<')?;
    let (body, closing) = match body.strip_prefix('/') {
        Some(rest) => (rest, true),
        None => (body, false),
    };
    let name_len = body
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == ':'))
        .unwrap_or(body.len());
    if name_len == 0 || !body.starts_with(|c: char| c.is_ascii_alphabetic()) {
        return None;
    }
    let close = body.find('>').map(|at| at + 1)?;
    let inside = &body[name_len..close - 1];
    let self_closing = inside.trim_end().ends_with('/');
    let end = start + 1 + usize::from(closing) + close;
    Some((
        Token::Tag(Tag {
            name: body[..name_len].to_ascii_lowercase(),
            closing,
            self_closing,
            attributes: attributes(inside.trim_end().trim_end_matches('/')),
            source: &html[start..end],
        }),
        end,
    ))
}

fn attributes(text: &str) -> Vec<(String, Option<&str>)> {
    let mut out = Vec::new();
    let mut rest = text.trim_start();
    while !rest.is_empty() {
        let name_len = rest
            .find(|c: char| c == '=' || c.is_whitespace())
            .unwrap_or(rest.len());
        if name_len == 0 {
            rest = rest[rest.chars().next().map_or(1, char::len_utf8)..].trim_start();
            continue;
        }
        let name = rest[..name_len].to_ascii_lowercase();
        let after = rest[name_len..].trim_start();
        let Some(after) = after.strip_prefix('=') else {
            out.push((name, None));
            rest = after;
            continue;
        };
        let after = after.trim_start();
        let (value, used) = value(after);
        out.push((name, Some(value)));
        rest = after[used..].trim_start();
    }
    out
}

fn value(text: &str) -> (&str, usize) {
    for quote in ['"', '\''] {
        if let Some(after) = text.strip_prefix(quote) {
            return match after.find(quote) {
                Some(end) => (&after[..end], end + 2),
                None => (after, text.len()),
            };
        }
    }
    let len = text.find(char::is_whitespace).unwrap_or(text.len());
    (&text[..len], len)
}

/// HTML text, escaped so it cannot re-open a tag.
pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '&' => out.push_str("&amp;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(ch),
        }
    }
    out
}

/// An attribute value, escaped for a double-quoted attribute.
pub fn escape_attribute(value: &str) -> String {
    escape(value).replace('\'', "&#39;")
}

#[cfg(test)]
mod tests;
