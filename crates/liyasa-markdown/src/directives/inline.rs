//! Inline directives `:name[content]{props}` (§7.5.1 item 5, CM-50).
//!
//! This is a pass over comrak's inline nodes rather than over the source, so
//! the content between the brackets is Markdown that has already been parsed:
//! `:note[see *this*]` keeps its emphasis. A directive may therefore begin in
//! one inline node and end in another, which is why the scan carries a stack
//! rather than working one text node at a time.
//!
//! An unterminated `:name[` is not an error; it is prose that happens to look
//! like a directive, and it is put back the way it was written.

use liyasa_core::document::{Inline, Props};

use super::props;

/// Replaces every inline directive in a parsed inline sequence, recursing into
/// the inline containers that can hold one.
pub fn scan(children: Vec<Inline>) -> Vec<Inline> {
    let mut out: Vec<Inline> = Vec::new();
    let mut stack: Vec<Open> = Vec::new();

    for child in children {
        match child {
            Inline::Text(text) => tokenize(&text, &mut out, &mut stack),
            Inline::Emph(inner) => push(&mut out, &mut stack, Inline::Emph(scan(inner))),
            Inline::Strong(inner) => push(&mut out, &mut stack, Inline::Strong(scan(inner))),
            Inline::Strike(inner) => push(&mut out, &mut stack, Inline::Strike(scan(inner))),
            Inline::Link {
                href,
                title,
                children,
                resolved,
            } => push(
                &mut out,
                &mut stack,
                Inline::Link {
                    href,
                    title,
                    children: scan(children),
                    resolved,
                },
            ),
            other => push(&mut out, &mut stack, other),
        }
    }

    // Whatever is still open was prose all along.
    while let Some(open) = stack.pop() {
        let undone = open.undo();
        let target = stack.last_mut().map_or(&mut out, |open| &mut open.children);
        target.extend(undone);
    }
    merge(out)
}

struct Open {
    name: String,
    children: Vec<Inline>,
}

impl Open {
    /// The literal text that opened this directive, followed by what it had
    /// collected.
    fn undo(self) -> Vec<Inline> {
        let mut out = vec![Inline::Text(format!(":{}[", self.name))];
        out.extend(self.children);
        out
    }
}

fn push(out: &mut Vec<Inline>, stack: &mut [Open], inline: Inline) {
    stack
        .last_mut()
        .map_or(out, |open| &mut open.children)
        .push(inline);
}

fn text(out: &mut Vec<Inline>, stack: &mut [Open], value: &str) {
    if value.is_empty() {
        return;
    }
    push(out, stack, Inline::Text(value.to_owned()));
}

fn tokenize(value: &str, out: &mut Vec<Inline>, stack: &mut Vec<Open>) {
    let bytes = value.as_bytes();
    let mut at = 0;
    let mut literal = 0;
    while at < bytes.len() {
        if bytes[at] == b']' && !stack.is_empty() {
            text(out, stack, &value[literal..at]);
            let (props, used) = props_after(&value[at + 1..]);
            let Some(open) = stack.pop() else {
                unreachable!("the stack is not empty")
            };
            push(
                out,
                stack,
                Inline::InlineComponent {
                    name: open.name,
                    props,
                    children: merge(open.children),
                },
            );
            at += 1 + used;
            literal = at;
            continue;
        }
        if let Some(name) = opens_at(value, at) {
            text(out, stack, &value[literal..at]);
            at += 1 + name.len() + 1;
            literal = at;
            stack.push(Open {
                name,
                children: Vec::new(),
            });
            continue;
        }
        at += 1;
    }
    text(out, stack, &value[literal..]);
}

/// `:name[` starting at `at`, where the colon is not part of a `::` leaf or a
/// `:::` container and the name is not empty.
fn opens_at(value: &str, at: usize) -> Option<String> {
    let bytes = value.as_bytes();
    if bytes[at] != b':' {
        return None;
    }
    if at > 0 && bytes[at - 1] == b':' {
        return None;
    }
    let rest = &value[at + 1..];
    let len = rest
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '_'))
        .unwrap_or(rest.len());
    if len == 0 || !rest[len..].starts_with('[') {
        return None;
    }
    Some(rest[..len].to_owned())
}

/// `{props}` immediately after the closing bracket, and how many bytes it took.
fn props_after(rest: &str) -> (Props, usize) {
    if !rest.starts_with('{') {
        return (Props::default(), 0);
    }
    let Some(end) = closing_brace(rest) else {
        return (Props::default(), 0);
    };
    let parsed = props::parse(&rest[..=end]);
    if parsed.errors.is_empty() {
        (parsed.props, end + 1)
    } else {
        (Props::default(), 0)
    }
}

/// The `}` that closes the brace at offset zero, ignoring quoted ones.
fn closing_brace(text: &str) -> Option<usize> {
    let mut quoted = false;
    for (at, ch) in text.char_indices().skip(1) {
        match ch {
            '"' => quoted = !quoted,
            '}' if !quoted => return Some(at),
            _ => {}
        }
    }
    None
}

/// Splitting a text node around a directive can leave neighbouring runs of
/// text; the AST has one text node per run.
fn merge(children: Vec<Inline>) -> Vec<Inline> {
    let mut out: Vec<Inline> = Vec::with_capacity(children.len());
    for child in children {
        match (out.last_mut(), child) {
            (Some(Inline::Text(before)), Inline::Text(after)) => before.push_str(&after),
            (_, child) => out.push(child),
        }
    }
    out
}

#[cfg(test)]
mod tests;
