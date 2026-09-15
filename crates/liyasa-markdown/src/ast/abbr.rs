//! Abbreviations (CM-40).
//!
//! `*[HTML]: HyperText Markup Language` defines one; every later occurrence of
//! `HTML` in the page's prose carries the expansion. comrak has no extension
//! for this, so the definition is taken out of the text before comrak sees it
//! and the expansion is applied to the parsed inlines afterwards.
//!
//! Matching is whole-word and case-sensitive, because `IT` and `it` are not the
//! same abbreviation, and longest-first, so a page that defines both `HTML` and
//! `HTML5` gets the one it meant.

use std::collections::BTreeMap;

use liyasa_core::document::{Block, Inline, Node, PropValue, Props};

/// The directive name an expanded abbreviation carries.
pub const ABBR: &str = "abbr";

/// Wraps every occurrence of a defined abbreviation.
pub fn apply(root: &mut Block, definitions: &BTreeMap<String, String>) {
    if definitions.is_empty() {
        return;
    }
    // Longest first, so `HTML5` wins over `HTML` where both are defined.
    let mut ordered: Vec<(&String, &String)> = definitions.iter().collect();
    ordered.sort_by(|a, b| b.0.len().cmp(&a.0.len()).then_with(|| a.0.cmp(b.0)));
    block(root, &ordered);
}

fn block(block_: &mut Block, definitions: &[(&String, &String)]) {
    // A code block's body is not prose.
    if matches!(
        block_.kind,
        liyasa_core::document::BlockKind::CodeBlock { .. }
            | liyasa_core::document::BlockKind::HtmlBlock { .. }
    ) {
        return;
    }
    let mut children = Vec::with_capacity(block_.children.len());
    for child in std::mem::take(&mut block_.children) {
        match child {
            Node::Block(mut child) => {
                block(&mut child, definitions);
                children.push(Node::Block(child));
            }
            Node::Inline(child) => {
                children.extend(inline(child, definitions).into_iter().map(Node::Inline));
            }
        }
    }
    block_.children = children;
}

fn inline(node: Inline, definitions: &[(&String, &String)]) -> Vec<Inline> {
    match node {
        Inline::Text(text) => split(&text, definitions),
        Inline::Emph(children) => vec![Inline::Emph(all(children, definitions))],
        Inline::Strong(children) => vec![Inline::Strong(all(children, definitions))],
        Inline::Strike(children) => vec![Inline::Strike(all(children, definitions))],
        Inline::Link {
            href,
            title,
            children,
            resolved,
        } => vec![Inline::Link {
            href,
            title,
            children: all(children, definitions),
            resolved,
        }],
        // An abbreviation inside an abbreviation would never terminate.
        other => vec![other],
    }
}

fn all(children: Vec<Inline>, definitions: &[(&String, &String)]) -> Vec<Inline> {
    children
        .into_iter()
        .flat_map(|child| inline(child, definitions))
        .collect()
}

fn split(text: &str, definitions: &[(&String, &String)]) -> Vec<Inline> {
    let Some((at, abbr, expansion)) = first_match(text, definitions) else {
        return vec![Inline::Text(text.to_owned())];
    };
    let mut out = Vec::new();
    if at > 0 {
        out.push(Inline::Text(text[..at].to_owned()));
    }
    let mut props = Props::default();
    props
        .0
        .insert("title".to_owned(), PropValue::Str((*expansion).clone()));
    out.push(Inline::InlineComponent {
        name: ABBR.to_owned(),
        props,
        children: vec![Inline::Text((*abbr).clone())],
    });
    out.extend(split(&text[at + abbr.len()..], definitions));
    out
}

/// The earliest whole-word occurrence of any definition, preferring the longest
/// where two start at the same place.
fn first_match<'a>(
    text: &str,
    definitions: &[(&'a String, &'a String)],
) -> Option<(usize, &'a String, &'a String)> {
    let mut best: Option<(usize, &String, &String)> = None;
    for (abbr, expansion) in definitions {
        let mut from = 0;
        while let Some(found) = text[from..].find(abbr.as_str()) {
            let at = from + found;
            if whole_word(text, at, abbr.len()) {
                if best.is_none_or(|(seen, _, _)| at < seen) {
                    best = Some((at, abbr, expansion));
                }
                break;
            }
            from = at + 1;
        }
    }
    best
}

fn whole_word(text: &str, at: usize, len: usize) -> bool {
    let before = text[..at].chars().next_back();
    let after = text[at + len..].chars().next();
    !before.is_some_and(is_word) && !after.is_some_and(is_word)
}

fn is_word(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_'
}

#[cfg(test)]
mod tests;
