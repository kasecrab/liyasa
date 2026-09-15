//! The plain-text serialization (CM-55).
//!
//! This is what the search index stores and what a screen reader would hear if
//! every component collapsed to its content. It contains no markup, no
//! directive syntax, and no component names: a reader searching for "install"
//! must not match a page because it happens to use the `install` component.

use liyasa_core::document::{Block, BlockKind, Inline, Node};

pub fn render(root: &Block) -> String {
    let mut out = String::new();
    block(root, &mut out);
    out.trim().to_owned()
}

fn block(block_: &Block, out: &mut String) {
    // A code block's body is text a reader searches for, but the fence is not.
    if let BlockKind::HtmlBlock { .. } = block_.kind {
        return;
    }
    let before = out.len();
    for child in &block_.children {
        match child {
            Node::Block(child) => block(child, out),
            Node::Inline(child) => inline(child, out),
        }
    }
    if let BlockKind::Component { slots, .. } = &block_.kind {
        for children in slots.0.values() {
            for child in children {
                match child {
                    Node::Block(child) => block(child, out),
                    Node::Inline(child) => inline(child, out),
                }
            }
        }
    }
    if out.len() > before && !out.ends_with('\n') {
        out.push('\n');
    }
}

fn inline(inline_: &Inline, out: &mut String) {
    match inline_ {
        Inline::Text(text) | Inline::Code(text) | Inline::Math(text) => out.push_str(text),
        Inline::Emph(children)
        | Inline::Strong(children)
        | Inline::Strike(children)
        | Inline::Link { children, .. }
        | Inline::InlineComponent { children, .. } => {
            for child in children {
                inline(child, out);
            }
        }
        // Alt text is what a reader would have seen.
        Inline::Image { alt, .. } => out.push_str(alt),
        Inline::SoftBreak | Inline::HardBreak => out.push(' '),
        // Raw HTML is markup, and a footnote marker is a number.
        Inline::HtmlInline(_) | Inline::FootnoteRef(_) | Inline::TemplateInline { .. } => {}
    }
}

#[cfg(test)]
mod tests;
