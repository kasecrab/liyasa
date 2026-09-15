//! Building a [`ComponentInst`] without spelling out every field.
//!
//! `liyasa-markdown` builds instances from the Rendered AST; the golden tests
//! and the editor's preview build them from nothing. Both go through here so a
//! later field on `ComponentInst` has one place to gain a default.

use liyasa_core::components::ComponentInst;
use liyasa_core::diagnostics::Diagnostic;
use liyasa_core::document::{Node, PropValue, Props, Slots};
use liyasa_core::ids::BlockId;

/// Points a diagnostic at the instance that caused it, when the instance came
/// from a source file at all.
pub fn located(diagnostic: Diagnostic, inst: &ComponentInst) -> Diagnostic {
    match inst.origin.span {
        Some(span) => diagnostic.at(span),
        None => diagnostic,
    }
}

#[derive(Debug, Clone)]
pub struct Builder {
    inst: ComponentInst,
}

/// Starts an instance of `name` with no props and no children.
pub fn new(name: &str) -> Builder {
    Builder {
        inst: ComponentInst {
            name: name.to_owned(),
            props: Props::default(),
            children: Vec::new(),
            slots: Slots::default(),
            id: BlockId::implicit("component", name, "", 0),
            origin: Default::default(),
        },
    }
}

impl Builder {
    pub fn prop(mut self, name: &str, value: impl Into<PropValue>) -> Self {
        self.inst.props.0.insert(name.to_owned(), value.into());
        self
    }

    pub fn props(mut self, props: Props) -> Self {
        self.inst.props = props;
        self
    }

    pub fn child(mut self, node: Node) -> Self {
        self.inst.children.push(node);
        self
    }

    pub fn children(mut self, nodes: impl IntoIterator<Item = Node>) -> Self {
        self.inst.children.extend(nodes);
        self
    }

    pub fn slot(mut self, name: &str, nodes: impl IntoIterator<Item = Node>) -> Self {
        self.inst
            .slots
            .0
            .insert(name.to_owned(), nodes.into_iter().collect());
        self
    }

    pub fn build(self) -> ComponentInst {
        self.inst
    }
}

/// Wraps a [`Builder`] so `nested(...)` reads as a child in a literal tree.
pub fn nested(builder: Builder) -> Node {
    crate::nodes::component(builder.build())
}
