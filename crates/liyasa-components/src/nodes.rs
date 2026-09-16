//! Building and reading the node trees a component receives as children.
//!
//! The constructors exist because a `Block` has five fields and a component's
//! children are written out literally in tests and by the editor's preview.

use liyasa_core::components::ComponentInst;
use liyasa_core::document::{Block, BlockKind, FenceAttrs, Inline, Node, Origin, Props, Slots};
use liyasa_core::ids::BlockId;

pub fn block(kind: BlockKind, children: Vec<Node>) -> Node {
    // Derived from what the block holds, not a constant: two fences on one
    // page with the same id are two elements a browser cannot tell apart, and
    // the copy button that names one copies the other.
    let id = BlockId::implicit("block", &crate::text::of(&children), "", 0);
    Node::Block(Block {
        id,
        explicit_id: None,
        kind,
        origin: Origin::default(),
        children,
    })
}

pub fn paragraph(text: &str) -> Node {
    block(
        BlockKind::Paragraph,
        vec![Node::Inline(Inline::Text(text.to_owned()))],
    )
}

pub fn paragraph_of(inlines: Vec<Inline>) -> Node {
    block(
        BlockKind::Paragraph,
        inlines.into_iter().map(Node::Inline).collect(),
    )
}

pub fn heading(level: u8, text: &str) -> Node {
    block(
        BlockKind::Heading {
            level,
            anchor: crate::anchor::slug(text),
        },
        vec![Node::Inline(Inline::Text(text.to_owned()))],
    )
}

pub fn code_block(lang: Option<&str>, body: &str) -> Node {
    code_block_with(lang, body, FenceAttrs::default())
}

pub fn code_block_with(lang: Option<&str>, body: &str, attrs: FenceAttrs) -> Node {
    block(
        BlockKind::CodeBlock {
            lang: lang.map(str::to_owned),
            attrs,
            highlighted: None,
        },
        vec![Node::Inline(Inline::Text(body.to_owned()))],
    )
}

pub fn component(inst: ComponentInst) -> Node {
    Node::Block(Block {
        id: inst.id,
        explicit_id: None,
        kind: BlockKind::Component {
            name: inst.name,
            props: inst.props,
            slots: inst.slots,
        },
        origin: inst.origin,
        children: inst.children,
    })
}

/// The inverse of [`component`]: a `Component` block read back as an instance.
pub fn as_component(node: &Node) -> Option<ComponentInst> {
    let Node::Block(block) = node else {
        return None;
    };
    let BlockKind::Component { name, props, slots } = &block.kind else {
        return None;
    };
    Some(ComponentInst {
        name: name.clone(),
        props: props.clone(),
        children: block.children.clone(),
        slots: slots.clone(),
        id: block.id,
        origin: block.origin.clone(),
    })
}

/// A component's direct children that are instances of `name` (or an alias).
pub fn component_children<'a>(
    children: &'a [Node],
    names: &'a [&'a str],
) -> impl Iterator<Item = ComponentInst> + 'a {
    children
        .iter()
        .filter_map(as_component)
        .filter(move |inst| names.contains(&inst.name.as_str()))
}

/// Whether the tree holds nothing a reader would see.
pub fn is_blank(nodes: &[Node]) -> bool {
    nodes.iter().all(|node| match node {
        Node::Inline(Inline::Text(text)) => text.trim().is_empty(),
        Node::Inline(Inline::SoftBreak | Inline::HardBreak) => true,
        Node::Block(block) => match block.kind {
            BlockKind::Paragraph => is_blank(&block.children),
            _ => false,
        },
        Node::Inline(_) => false,
    })
}

/// An empty instance, for a component that renders with no children.
pub fn empty_inst(name: &str) -> ComponentInst {
    ComponentInst {
        name: name.to_owned(),
        props: Props::default(),
        children: Vec::new(),
        slots: Slots::default(),
        id: BlockId::implicit("component", name, "", 0),
        origin: Origin::default(),
    }
}
