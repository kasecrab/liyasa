//! Plain text for the search index (§34.9 `Component::render_text`).
//!
//! Pure: the same node tree always yields the same string, with no context and
//! no allocation of intermediate markup. Block boundaries become one space, so
//! a phrase never merges across a paragraph break.

use liyasa_core::document::{Block, BlockKind, Inline, Node};

/// The readable text of a node tree, whitespace collapsed.
pub fn of(nodes: &[Node]) -> String {
    let mut out = String::new();
    for node in nodes {
        push_node(node, &mut out);
    }
    collapse(&out)
}

/// The text of one instance's children, prefixed by whatever the component
/// itself contributes (a title, a caption).
pub fn with_titles(titles: &[&str], nodes: &[Node]) -> String {
    let mut out = String::new();
    for title in titles {
        out.push_str(title);
        out.push(' ');
    }
    for node in nodes {
        push_node(node, &mut out);
    }
    collapse(&out)
}

fn push_node(node: &Node, out: &mut String) {
    match node {
        Node::Block(block) => push_block(block, out),
        Node::Inline(inline) => push_inline(inline, out),
    }
}

fn push_block(block: &Block, out: &mut String) {
    match &block.kind {
        // A code block's body is indexed; its language is not.
        BlockKind::CodeBlock { .. } | BlockKind::Math { .. } => {}
        // Raw HTML is not text until it is parsed, and parsing it here would
        // index tag names.
        BlockKind::HtmlBlock { .. } => return,
        BlockKind::Component { name, .. } => {
            out.push(' ');
            let _ = name;
        }
        _ => {}
    }
    if let BlockKind::Math { src, .. } = &block.kind {
        out.push_str(src);
    }
    for child in &block.children {
        push_node(child, out);
    }
    out.push(' ');
}

fn push_inline(inline: &Inline, out: &mut String) {
    match inline {
        Inline::Text(text) | Inline::Code(text) | Inline::Math(text) => out.push_str(text),
        Inline::Emph(children) | Inline::Strong(children) | Inline::Strike(children) => {
            for child in children {
                push_inline(child, out);
            }
        }
        Inline::Link { children, .. } => {
            for child in children {
                push_inline(child, out);
            }
        }
        Inline::InlineComponent { children, .. } => {
            for child in children {
                push_inline(child, out);
            }
        }
        // Alt text is what a reader of the page would have.
        Inline::Image { alt, .. } => out.push_str(alt),
        Inline::SoftBreak | Inline::HardBreak => out.push(' '),
        Inline::HtmlInline(_) | Inline::FootnoteRef(_) | Inline::TemplateInline { .. } => {}
    }
}

/// Whitespace runs to one space, edges trimmed.
pub fn collapse(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        if ch.is_whitespace() {
            if !out.ends_with(' ') {
                out.push(' ');
            }
        } else {
            out.push(ch);
        }
    }
    out.trim().to_owned()
}

#[cfg(test)]
mod tests {
    use crate::nodes;

    use super::*;

    #[test]
    fn blocks_do_not_run_together() {
        let tree = vec![nodes::paragraph("one"), nodes::paragraph("two")];
        assert_eq!(of(&tree), "one two");
    }

    #[test]
    fn a_code_block_body_is_indexed() {
        let tree = vec![nodes::code_block(Some("rust"), "fn main() {}")];
        assert_eq!(of(&tree), "fn main() {}");
    }

    #[test]
    fn image_alt_text_stands_in_for_the_image() {
        let tree = vec![nodes::paragraph_of(vec![Inline::Image {
            src: "a.png".into(),
            alt: "A chart".into(),
            title: None,
            dark: None,
        }])];
        assert_eq!(of(&tree), "A chart");
    }

    #[test]
    fn raw_html_contributes_no_tag_names() {
        let tree = vec![nodes::block(
            BlockKind::HtmlBlock {
                html: "<div>x</div>".into(),
            },
            Vec::new(),
        )];
        assert_eq!(of(&tree), "");
    }

    #[test]
    fn titles_come_before_the_body() {
        let tree = vec![nodes::paragraph("body")];
        assert_eq!(with_titles(&["Heading"], &tree), "Heading body");
    }
}
