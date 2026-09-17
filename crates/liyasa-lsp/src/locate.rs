//! What is under the cursor, for the requests that ask about a name already
//! written rather than one being typed.
//!
//! Completion reads the text *before* the cursor, because that is all an author
//! has typed. Hover and go-to-definition read the whole token the cursor is in,
//! in both directions. The two are deliberately separate: sharing one parser
//! between them would make every completion depend on text the author has not
//! written yet.

use crate::text::Text;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// A directive's name: `note` in `:::note{…}`.
    Component { name: String, span: (u32, u32) },
    /// A prop's name on a known component.
    Prop {
        component: String,
        name: String,
        span: (u32, u32),
    },
    /// A dotted name in a template expression, which is a variable or a fact
    /// depending on what the workspace holds.
    Path { path: String, span: (u32, u32) },
    /// A snippet named in an `include`.
    Snippet { name: String, span: (u32, u32) },
    /// A route, in a link target or a `Route`-typed prop.
    Route { route: String, span: (u32, u32) },
}

impl Target {
    pub fn span(&self) -> (u32, u32) {
        match self {
            Self::Component { span, .. }
            | Self::Prop { span, .. }
            | Self::Path { span, .. }
            | Self::Snippet { span, .. }
            | Self::Route { span, .. } => *span,
        }
    }
}

pub fn at(text: &Text, offset: u32) -> Option<Target> {
    let source = text.as_str();
    let line_start = source
        .get(..offset as usize)?
        .rfind('\n')
        .map_or(0, |at| at + 1);
    let line_end = source[line_start..]
        .find('\n')
        .map_or(source.len(), |at| line_start + at);
    let line = &source[line_start..line_end];
    let column = offset as usize - line_start;
    let base = u32::try_from(line_start).ok()?;

    if let Some(target) = in_template(line, column, base) {
        return Some(target);
    }
    if let Some(target) = in_directive(line, column, base) {
        return Some(target);
    }
    in_link(line, column, base)
}

/// The `{{ … }}` or `{% … %}` the cursor is inside, if it is inside one.
fn in_template(line: &str, column: usize, base: u32) -> Option<Target> {
    let (open, close) = enclosing(line, column, &[("{{", "}}"), ("{%", "%}")])?;
    let inner = &line[open..close];

    // `include "legal/terms"` — the cursor in the quotes names a snippet.
    if let Some((quoted_start, quoted_end)) = quoted_after_keyword(inner)
        && (open + quoted_start..=open + quoted_end).contains(&column)
    {
        return Some(Target::Snippet {
            name: inner[quoted_start..quoted_end].to_owned(),
            span: span(base, open + quoted_start, open + quoted_end),
        });
    }

    let (start, end) = word_at(inner, column.saturating_sub(open), is_path_char)?;
    Some(Target::Path {
        path: inner[start..end].to_owned(),
        span: span(base, open + start, open + end),
    })
}

/// A directive occupies its whole line, after any container or list indent.
fn in_directive(line: &str, column: usize, base: u32) -> Option<Target> {
    let indent = line.len() - line.trim_start_matches([' ', '\t', '>']).len();
    let body = &line[indent..];
    let colons = body.bytes().take_while(|b| *b == b':').count();
    if colons == 0 || colons > 4 {
        return None;
    }
    let name_start = indent + colons;
    let name_len = line[name_start..]
        .bytes()
        .take_while(|b| b.is_ascii_alphanumeric() || *b == b'-' || *b == b'_')
        .count();
    if name_len == 0 {
        return None;
    }
    let name_end = name_start + name_len;
    let name = &line[name_start..name_end];

    if (name_start..=name_end).contains(&column) {
        return Some(Target::Component {
            name: name.to_owned(),
            span: span(base, name_start, name_end),
        });
    }

    let (open, close) = enclosing(line, column, &[("{", "}")])?;
    if open < name_end {
        return None;
    }
    let inner = &line[open..close];
    let at = column.saturating_sub(open);

    // Left of the `=` is the prop; right of it is a value, and a route value is
    // a route wherever it is written.
    let (start, end) = word_at(inner, at, |c| c.is_alphanumeric() || c == '-' || c == '_')?;
    if inner[end..].starts_with('=') {
        return Some(Target::Prop {
            component: name.to_owned(),
            name: inner[start..end].to_owned(),
            span: span(base, open + start, open + end),
        });
    }

    let (start, end) = word_at(inner, at, |c| !matches!(c, '"' | '\'' | ' ' | '\t'))?;
    let value = &inner[start..end];
    value.starts_with('/').then(|| Target::Route {
        route: value.to_owned(),
        span: span(base, open + start, open + end),
    })
}

fn in_link(line: &str, column: usize, base: u32) -> Option<Target> {
    let (open, close) = enclosing(line, column, &[("](", ")")])?;
    let target = &line[open..close];
    (!target.is_empty()).then(|| Target::Route {
        route: target.to_owned(),
        span: span(base, open, close),
    })
}

/// The innermost pair of delimiters the column sits between, as byte offsets of
/// the text inside them.
fn enclosing(line: &str, column: usize, pairs: &[(&str, &str)]) -> Option<(usize, usize)> {
    let mut best: Option<(usize, usize)> = None;
    for (open, close) in pairs {
        let mut at = 0;
        while let Some(found) = line[at..].find(open) {
            let inner = at + found + open.len();
            let Some(end) = line[inner..].find(close).map(|end| inner + end) else {
                break;
            };
            if (inner..=end).contains(&column) {
                best = Some(match best {
                    Some((b_open, b_close)) if b_close - b_open <= end - inner => (b_open, b_close),
                    _ => (inner, end),
                });
            }
            at = inner;
        }
    }
    best
}

/// The run of characters satisfying `is_part` that the column is inside.
fn word_at(text: &str, column: usize, is_part: impl Fn(char) -> bool) -> Option<(usize, usize)> {
    let column = column.min(text.len());
    let start = text[..column]
        .char_indices()
        .rev()
        .take_while(|(_, c)| is_part(*c))
        .last()
        .map_or(column, |(at, _)| at);
    let end = text[column..]
        .char_indices()
        .find(|(_, c)| !is_part(*c))
        .map_or(text.len(), |(at, _)| column + at);
    (start < end).then_some((start, end))
}

fn is_path_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '.'
}

/// The quoted argument of `include`, `import`, `from` or `extends`, as offsets
/// into the statement's body.
fn quoted_after_keyword(inner: &str) -> Option<(usize, usize)> {
    let trimmed = inner.trim_start();
    let indent = inner.len() - trimmed.len();
    let keyword = ["include", "import", "from", "extends"]
        .iter()
        .find(|keyword| trimmed.starts_with(*keyword))?;
    let rest = &trimmed[keyword.len()..];
    let open = rest.find(['"', '\''])?;
    let quote = rest.as_bytes()[open] as char;
    let close = rest[open + 1..].find(quote)? + open + 1;
    Some((
        indent + keyword.len() + open + 1,
        indent + keyword.len() + close,
    ))
}

fn span(base: u32, start: usize, end: usize) -> (u32, u32) {
    (
        base + u32::try_from(start).unwrap_or(0),
        base + u32::try_from(end).unwrap_or(0),
    )
}
