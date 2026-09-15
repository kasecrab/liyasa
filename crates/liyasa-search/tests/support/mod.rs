//! Rendered-AST fixtures. The real builder is `liyasa-markdown`'s; these hand
//! a section extractor the same shapes without taking a dependency on it.

use liyasa_core::document::{
    Block, BlockKind, Deps, Document, FenceAttrs, Inline, Node, Origin, Props, Slots,
};
use liyasa_core::{BlockId, Diagnostics};

pub fn block(kind: BlockKind, children: Vec<Node>) -> Node {
    Node::Block(Block {
        id: BlockId::implicit("fixture", &format!("{kind:?}"), "", 0),
        explicit_id: None,
        kind,
        origin: Origin {
            span: None,
            frames: Vec::new(),
        },
        children,
    })
}

pub fn text(value: &str) -> Node {
    Node::Inline(Inline::Text(value.to_owned()))
}

pub fn heading(level: u8, anchor: &str, title: &str) -> Node {
    block(
        BlockKind::Heading {
            level,
            anchor: anchor.to_owned(),
        },
        vec![text(title)],
    )
}

pub fn para(value: &str) -> Node {
    block(BlockKind::Paragraph, vec![text(value)])
}

pub fn fence(lang: &str, body: &str) -> Node {
    block(
        BlockKind::CodeBlock {
            lang: Some(lang.to_owned()),
            attrs: FenceAttrs::default(),
            highlighted: None,
        },
        vec![text(body)],
    )
}

pub fn component(name: &str, props: Props, children: Vec<Node>) -> Node {
    block(
        BlockKind::Component {
            name: name.to_owned(),
            props,
            slots: Slots::default(),
        },
        children,
    )
}

pub fn document(children: Vec<Node>) -> Document {
    let Node::Block(root) = block(BlockKind::Document, children) else {
        unreachable!("block() returns a block")
    };
    Document {
        root,
        deps: Deps::default(),
        diagnostics: Diagnostics::new(),
    }
}
