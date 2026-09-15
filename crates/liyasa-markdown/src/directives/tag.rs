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

// ---- re-nesting ----
//
// comrak reports `<Card>`, the content, and `</Card>` as three siblings,
// because to comrak they are HTML. Folding them back into one component is a
// pass over a sibling list, block and inline alike. An unmatched tag stays what
// comrak made of it: prose that happens to look like a tag is not an error.

use liyasa_core::Origin;
use liyasa_core::document::{Block, BlockKind, Inline, Node, Slots};
use liyasa_core::ids::BlockId;

/// Folds component tags in a block sibling list into component blocks.
pub fn nest_blocks(nodes: Vec<Node>) -> Vec<Node> {
    let mut out: Vec<Node> = Vec::new();
    let mut open: Vec<(Tag, Origin, Vec<Node>)> = Vec::new();

    for node in nodes {
        let Some(tag) = block_tag(&node) else {
            collect(&mut out, &mut open, node);
            continue;
        };
        let origin = match &node {
            Node::Block(block) => block.origin.clone(),
            Node::Inline(_) => Origin::default(),
        };
        match tag.form {
            Form::SelfClosing => {
                collect(
                    &mut out,
                    &mut open,
                    Node::Block(component(tag, origin, Vec::new())),
                );
            }
            Form::Open => open.push((tag, origin, Vec::new())),
            Form::Close => match close_at(&open, &tag.name) {
                Some(at) => {
                    // Anything still open inside was never closed; it goes back
                    // to being what comrak made of it.
                    while open.len() > at + 1 {
                        let Some((tag, origin, children)) = open.pop() else {
                            break;
                        };
                        let undone = undo(tag, origin, children);
                        let target = open.last_mut().map_or(&mut out, |(_, _, held)| held);
                        target.extend(undone);
                    }
                    let Some((tag, origin, children)) = open.pop() else {
                        continue;
                    };
                    collect(
                        &mut out,
                        &mut open,
                        Node::Block(component(tag, origin, children)),
                    );
                }
                None => collect(&mut out, &mut open, node),
            },
        }
    }

    while let Some((tag, origin, children)) = open.pop() {
        let undone = undo(tag, origin, children);
        let target = open.last_mut().map_or(&mut out, |(_, _, held)| held);
        target.extend(undone);
    }
    out
}

/// Folds component tags in an inline sibling list into inline components.
pub fn nest_inlines(inlines: Vec<Inline>) -> Vec<Inline> {
    let mut out: Vec<Inline> = Vec::new();
    let mut open: Vec<(Tag, Vec<Inline>)> = Vec::new();

    for inline in inlines {
        let Inline::HtmlInline(html) = &inline else {
            open.last_mut()
                .map_or(&mut out, |(_, held)| held)
                .push(inline);
            continue;
        };
        let Some(tag) = parse(html) else {
            open.last_mut()
                .map_or(&mut out, |(_, held)| held)
                .push(inline);
            continue;
        };
        match tag.form {
            Form::SelfClosing => {
                let component = Inline::InlineComponent {
                    name: tag.name,
                    props: tag.props,
                    children: Vec::new(),
                };
                open.last_mut()
                    .map_or(&mut out, |(_, held)| held)
                    .push(component);
            }
            Form::Open => open.push((tag, Vec::new())),
            Form::Close => {
                let at = open.iter().rposition(|(held, _)| held.name == tag.name);
                let Some(at) = at else {
                    open.last_mut()
                        .map_or(&mut out, |(_, held)| held)
                        .push(inline);
                    continue;
                };
                while open.len() > at + 1 {
                    let Some((tag, children)) = open.pop() else {
                        break;
                    };
                    let undone = undo_inline(tag, children);
                    open.last_mut()
                        .map_or(&mut out, |(_, held)| held)
                        .extend(undone);
                }
                let Some((tag, children)) = open.pop() else {
                    continue;
                };
                let component = Inline::InlineComponent {
                    name: tag.name,
                    props: tag.props,
                    children,
                };
                open.last_mut()
                    .map_or(&mut out, |(_, held)| held)
                    .push(component);
            }
        }
    }

    while let Some((tag, children)) = open.pop() {
        let undone = undo_inline(tag, children);
        open.last_mut()
            .map_or(&mut out, |(_, held)| held)
            .extend(undone);
    }
    out
}

/// The component tag an HTML block is, if it is one.
fn block_tag(node: &Node) -> Option<Tag> {
    match node {
        Node::Block(Block {
            kind: BlockKind::HtmlBlock { html },
            ..
        }) => parse(html),
        _ => None,
    }
}

fn collect(out: &mut Vec<Node>, open: &mut [(Tag, Origin, Vec<Node>)], node: Node) {
    open.last_mut().map_or(out, |(_, _, held)| held).push(node);
}

fn close_at(open: &[(Tag, Origin, Vec<Node>)], name: &str) -> Option<usize> {
    open.iter().rposition(|(tag, _, _)| tag.name == name)
}

fn component(tag: Tag, origin: Origin, children: Vec<Node>) -> Block {
    Block {
        id: BlockId([0; 12]),
        explicit_id: None,
        kind: BlockKind::Component {
            name: tag.name,
            props: tag.props,
            slots: Slots::default(),
        },
        origin,
        children,
    }
}

/// An unmatched open tag was raw HTML all along.
fn undo(tag: Tag, origin: Origin, children: Vec<Node>) -> Vec<Node> {
    let mut out = vec![Node::Block(Block {
        id: BlockId([0; 12]),
        explicit_id: None,
        kind: BlockKind::HtmlBlock {
            html: reconstruct(&tag),
        },
        origin,
        children: Vec::new(),
    })];
    out.extend(children);
    out
}

fn undo_inline(tag: Tag, children: Vec<Inline>) -> Vec<Inline> {
    let mut out = vec![Inline::HtmlInline(reconstruct(&tag))];
    out.extend(children);
    out
}

fn reconstruct(tag: &Tag) -> String {
    use std::fmt::Write as _;
    let mut out = format!("<{}", tag.name);
    for (name, value) in &tag.props.0 {
        match value {
            PropValue::Bool(true) => {
                let _ = write!(out, " {name}");
            }
            PropValue::Str(text) => {
                let _ = write!(out, " {name}=\"{text}\"");
            }
            PropValue::Expr(expr) => {
                let _ = write!(out, " {name}={{{{ {expr} }}}}");
            }
            other => {
                let _ = write!(out, " {name}={}", super::render_value(other));
            }
        }
    }
    out.push('>');
    out
}
