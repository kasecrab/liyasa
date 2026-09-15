//! Named slots (CM-52).
//!
//! `:::slot{name="footer"}` inside a component body is not a child of that
//! component; it is a second body for it. Lifting it out here means every
//! renderer and every serializer sees slots the same way, and a component
//! implementation never has to filter its own children.

use liyasa_core::diagnostics::code;
use liyasa_core::document::{Block, BlockKind, Node, PropValue};
use liyasa_core::{Diagnostic, Diagnostics};

/// The directive name that declares a slot.
pub const SLOT: &str = "slot";

/// Moves every slot into the component that holds it.
pub fn lift(root: &mut Block, diagnostics: &mut Diagnostics) {
    let inside_component = matches!(root.kind, BlockKind::Component { .. });
    for child in &mut root.children {
        if let Node::Block(child) = child {
            lift(child, diagnostics);
        }
    }

    let mut lifted: Vec<(String, Vec<Node>)> = Vec::new();
    let mut kept: Vec<Node> = Vec::with_capacity(root.children.len());
    for child in std::mem::take(&mut root.children) {
        let Some(name) = slot_name(&child) else {
            kept.push(child);
            continue;
        };
        let Node::Block(block) = child else {
            kept.push(child);
            continue;
        };
        match name {
            _ if !inside_component => {
                diagnostics.push(located(
                    Diagnostic::new(
                        code::E0350,
                        "a slot only means something inside a component body",
                    )
                    .help("move it inside the `:::component` it belongs to"),
                    &block,
                ));
                kept.extend(block.children);
            }
            Some(name) => lifted.push((name, block.children)),
            None => {
                diagnostics.push(located(
                    Diagnostic::new(code::E0350, "a slot needs a name")
                        .help(r#"write `:::slot{name="footer"}`"#),
                    &block,
                ));
                kept.extend(block.children);
            }
        }
    }
    root.children = kept;

    if let BlockKind::Component { slots, .. } = &mut root.kind {
        for (name, children) in lifted {
            slots.0.entry(name).or_default().extend(children);
        }
    }
}

/// `Some(Some(name))` for a named slot, `Some(None)` for one without a name,
/// and `None` for anything that is not a slot.
fn slot_name(node: &Node) -> Option<Option<String>> {
    let Node::Block(Block {
        kind: BlockKind::Component { name, props, .. },
        ..
    }) = node
    else {
        return None;
    };
    if name != SLOT {
        return None;
    }
    Some(match props.get("name") {
        Some(PropValue::Str(name)) if !name.is_empty() => Some(name.clone()),
        _ => None,
    })
}

fn located(diagnostic: Diagnostic, block: &Block) -> Diagnostic {
    match block.origin.span {
        Some(span) => diagnostic.at(span),
        None => diagnostic,
    }
}

#[cfg(test)]
mod tests;
