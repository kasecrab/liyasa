//! Block identity and heading anchors (PRD §7.16, CM-31).
//!
//! One ordered walk, because every answer here depends on what came before:
//! an anchor on the nearest preceding heading, a counter among identical
//! siblings, and the set of explicit IDs already claimed.

use std::collections::BTreeMap;

use liyasa_core::diagnostics::code;
use liyasa_core::document::{Block, BlockKind, Inline, Node, PropValue};
use liyasa_core::ids::BlockId;
use liyasa_core::{Diagnostic, Diagnostics, Span};

use super::anchors::Anchors;
use super::build::{normalize, plain};

/// Assigns every anchor and every block ID, in document order.
pub fn assign(root: &mut Block, diagnostics: &mut Diagnostics) {
    let mut pass = Pass {
        anchors: Anchors::new(),
        ordinals: BTreeMap::new(),
        claimed: BTreeMap::new(),
        heading: String::new(),
        diagnostics,
    };
    pass.block(root);
}

struct Pass<'a> {
    anchors: Anchors,
    /// How many blocks with the same kind, text, and heading have been seen.
    ordinals: BTreeMap<(String, String, String), u32>,
    claimed: BTreeMap<String, Option<Span>>,
    heading: String,
    diagnostics: &'a mut Diagnostics,
}

impl Pass<'_> {
    fn block(&mut self, block: &mut Block) {
        let explicit = self.explicit_of(block);
        if let Some(id) = &explicit
            && let Some(first) = self.claimed.insert(id.clone(), block.origin.span)
        {
            let mut diagnostic = Diagnostic::new(
                code::E0318,
                format!("explicit block id `{id}` is used more than once on this page"),
            )
            .help("an ID is a link target, so two blocks cannot share one");
            if let Some(span) = block.origin.span {
                diagnostic = diagnostic.at(span);
            }
            if let Some(span) = first {
                diagnostic = diagnostic.label(span, "first used here");
            }
            self.diagnostics.push(diagnostic);
        }

        if let BlockKind::Heading { anchor, .. } = &mut block.kind {
            let text = plain_of(&block.children);
            let assigned = self.anchors.assign(&text, explicit.as_deref());
            *anchor = assigned.clone();
            self.heading = assigned;
        }

        block.id = match &explicit {
            Some(id) => BlockId::explicit(id),
            None => {
                let kind = kind_name(&block.kind);
                let text = normalize(&plain_of(&block.children));
                let key = (kind.to_owned(), text.clone(), self.heading.clone());
                let ordinal = self.ordinals.entry(key).or_default();
                let id = BlockId::implicit(kind, &text, &self.heading, *ordinal);
                *ordinal += 1;
                id
            }
        };
        block.explicit_id = explicit;

        for child in &mut block.children {
            if let Node::Block(child) = child {
                self.block(child);
            }
        }
    }

    /// The `{#id}` an author attached, from a component's props or from the
    /// end of the block's own text.
    fn explicit_of(&mut self, block: &mut Block) -> Option<String> {
        if let BlockKind::Component { props, .. } = &block.kind
            && let Some(PropValue::Str(id)) = props.get("id")
        {
            return Some(id.clone());
        }
        take_trailing_id(&mut block.children)
    }
}

/// Strips a trailing `{#id}` from the block's own last text run and returns it.
///
/// It never looks inside a child block: the ID belongs to the block whose text
/// carries it, so `- item {#first}` names the paragraph inside the item rather
/// than the list, the document, and the item all at once.
///
/// `{.class}` is left where it is: a class is not an identity, and the corpus
/// asserts it stays literal.
pub fn take_trailing_id(children: &mut Vec<Node>) -> Option<String> {
    let at = children.iter().rposition(is_text)?;
    if children[at + 1..].iter().any(|node| !is_text(node)) {
        return None;
    }
    let Some(Node::Inline(Inline::Text(text))) = children.get_mut(at) else {
        return None;
    };
    let id = split_trailing_id(text)?;
    if text.is_empty() {
        children.remove(at);
    }
    Some(id)
}

fn is_text(node: &Node) -> bool {
    matches!(node, Node::Inline(Inline::Text(_)))
}

/// `Title {#anchor}` to `Title`, returning `anchor`.
fn split_trailing_id(text: &mut String) -> Option<String> {
    let trimmed = text.trim_end();
    let body = trimmed.strip_suffix('}')?;
    let at = body.rfind('{')?;
    let id = body[at + 1..].strip_prefix('#')?;
    if id.is_empty() || !id.chars().all(is_id_char) {
        return None;
    }
    let id = id.to_owned();
    text.truncate(at);
    let kept = text.trim_end().len();
    text.truncate(kept);
    Some(id)
}

fn is_id_char(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '-' || ch == '_'
}

fn plain_of(children: &[Node]) -> String {
    let inlines: Vec<Inline> = children
        .iter()
        .filter_map(|node| match node {
            Node::Inline(inline) => Some(inline.clone()),
            Node::Block(_) => None,
        })
        .collect();
    if inlines.is_empty() {
        return children
            .iter()
            .filter_map(|node| match node {
                Node::Block(block) => Some(plain_of(&block.children)),
                Node::Inline(_) => None,
            })
            .collect::<Vec<_>>()
            .join(" ");
    }
    plain(&inlines)
}

/// The discriminant that goes into a block's identity. Two blocks of different
/// kinds with the same text are different blocks.
pub fn kind_name(kind: &BlockKind) -> &'static str {
    match kind {
        BlockKind::Document => "document",
        BlockKind::Heading { .. } => "heading",
        BlockKind::Paragraph => "paragraph",
        BlockKind::List { .. } => "list",
        BlockKind::ListItem { .. } => "list-item",
        BlockKind::BlockQuote => "block-quote",
        BlockKind::CodeBlock { .. } => "code-block",
        BlockKind::HtmlBlock { .. } => "html-block",
        BlockKind::Table { .. } => "table",
        BlockKind::TableRow { .. } => "table-row",
        BlockKind::TableCell => "table-cell",
        BlockKind::ThematicBreak => "thematic-break",
        BlockKind::FootnoteDefinition { .. } => "footnote-definition",
        BlockKind::Math { .. } => "math",
        BlockKind::Component { .. } => "component",
        BlockKind::LogicMarker { .. } => "logic-marker",
    }
}
