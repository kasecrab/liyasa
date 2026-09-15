//! The HTML serialization (§34.9).
//!
//! The theme reaches this module through [`Blocks`] rather than through
//! `Renderer` directly. The two methods of `Renderer` that matter here both
//! need a `&mut RenderCtx`, which no crate outside `liyasa-core` can construct
//! (`plan/rfcs/0025-render-ctx-is-unconstructible.md`), so a renderer written
//! against `Renderer` could not be called by a test — or by anything else. The
//! seam here is one the caller closes over its own context, which leaves the
//! adapter in `super` as the only untestable line in the module.
//!
//! The structural nodes are rendered here because their HTML is the same under
//! every theme; components and code blocks go to the `Renderer`, because theirs
//! is not. Text is escaped on the way out, and raw HTML is emitted verbatim
//! because the sanitizer has already run over the tree — that ordering is the
//! whole reason `content.html` is enforced after parsing rather than during it.

use std::fmt::Write as _;

use liyasa_core::components::{ComponentInst, RenderError};
use liyasa_core::document::{Align, Block, BlockKind, Inline, Node};

use crate::sanitize::html::escape;

/// What the theme owns: the two node kinds whose HTML it decides.
///
/// A theme that fails on one does not fail the page — the content is emitted in
/// a neutral shape instead, because a reader is better served by an unstyled
/// callout than by a hole where one was.
pub trait Blocks {
    fn component(&mut self, inst: &ComponentInst, children: &str) -> Result<String, RenderError>;
    fn code(&mut self, block: &Block, body: &str) -> Result<String, RenderError>;
}

pub fn render(root: &Block, theme: &mut dyn Blocks) -> String {
    let mut out = String::new();
    block(root, theme, &mut out);
    out
}

fn block(block_: &Block, theme: &mut dyn Blocks, out: &mut String) {
    match &block_.kind {
        BlockKind::Document => children(block_, theme, out),
        BlockKind::Heading { level, anchor } => {
            let _ = write!(out, "<h{level} id=\"{}\">", escape(anchor));
            children(block_, theme, out);
            let _ = writeln!(out, "</h{level}>");
        }
        BlockKind::Paragraph => {
            out.push_str(&open("p", block_));
            children(block_, theme, out);
            out.push_str("</p>\n");
        }
        BlockKind::BlockQuote => wrapped("blockquote", block_, theme, out),
        BlockKind::List {
            ordered,
            start,
            tight,
        } => {
            let tag = if *ordered { "ol" } else { "ul" };
            if *ordered && *start != 1 {
                let _ = writeln!(out, "<{tag} start=\"{start}\">");
            } else {
                let _ = writeln!(out, "<{tag}>");
            }
            for child in &block_.children {
                match child {
                    Node::Block(item) => item_html(item, *tight, theme, out),
                    Node::Inline(child) => inline(child, theme, out),
                }
            }
            let _ = writeln!(out, "</{tag}>");
        }
        BlockKind::ListItem { .. } => item_html(block_, false, theme, out),
        BlockKind::CodeBlock { highlighted, .. } => {
            let body = highlighted
                .clone()
                .unwrap_or_else(|| escape(&code_of(block_)));
            match theme.code(block_, &body) {
                Ok(html) => out.push_str(&html),
                Err(_) => {
                    let _ = writeln!(out, "<pre><code>{body}</code></pre>");
                }
            }
        }
        // The sanitizer has already decided what may be here.
        BlockKind::HtmlBlock { html } => out.push_str(html),
        BlockKind::Table { align } => {
            out.push_str("<table>\n");
            table(block_, align, theme, out);
            out.push_str("</table>\n");
        }
        BlockKind::TableRow { .. } | BlockKind::TableCell => children(block_, theme, out),
        BlockKind::ThematicBreak => out.push_str("<hr />\n"),
        BlockKind::FootnoteDefinition { label } => {
            let _ = writeln!(out, "<li id=\"fn-{}\">", escape(label));
            children(block_, theme, out);
            out.push_str("</li>\n");
        }
        BlockKind::Math { display, src } => {
            let tag = if *display { "div" } else { "span" };
            let _ = writeln!(out, "<{tag} class=\"math\">{}</{tag}>", escape(src));
        }
        BlockKind::Component { name, props, slots } => {
            let mut inner = String::new();
            children(block_, theme, &mut inner);
            let inst = ComponentInst {
                name: name.clone(),
                props: props.clone(),
                children: block_.children.clone(),
                slots: slots.clone(),
                id: block_.id,
                origin: block_.origin.clone(),
            };
            match theme.component(&inst, &inner) {
                Ok(html) => out.push_str(&html),
                // A theme that cannot render a component must not lose its
                // content: the page is degraded, not truncated.
                Err(_) => {
                    let _ = writeln!(out, "<div class=\"{}\">", escape(name));
                    out.push_str(&inner);
                    out.push_str("</div>\n");
                }
            }
        }
        // Dev only, and never part of a built page.
        BlockKind::LogicMarker { .. } => {}
    }
}

/// A tight list item's paragraphs are not wrapped, which is what makes a list
/// tight in CommonMark's output.
fn item_html(item: &Block, tight: bool, theme: &mut dyn Blocks, out: &mut String) {
    let BlockKind::ListItem { checked } = &item.kind else {
        block(item, theme, out);
        return;
    };
    out.push_str(&open("li", item));
    if let Some(checked) = checked {
        let _ = write!(
            out,
            "<input type=\"checkbox\" disabled{} /> ",
            if *checked { " checked" } else { "" }
        );
    }
    for child in &item.children {
        match child {
            Node::Block(child) if tight && matches!(child.kind, BlockKind::Paragraph) => {
                children(child, theme, out);
            }
            Node::Block(child) => block(child, theme, out),
            Node::Inline(child) => inline(child, theme, out),
        }
    }
    out.push_str("</li>\n");
}

fn table(block_: &Block, align: &[Align], theme: &mut dyn Blocks, out: &mut String) {
    let mut section: Option<&'static str> = None;
    for child in &block_.children {
        let Node::Block(row) = child else { continue };
        let BlockKind::TableRow { header } = row.kind else {
            continue;
        };
        let wanted = if header { "thead" } else { "tbody" };
        if section != Some(wanted) {
            if let Some(open) = section {
                let _ = writeln!(out, "</{open}>");
            }
            let _ = writeln!(out, "<{wanted}>");
            section = Some(wanted);
        }
        out.push_str("<tr>\n");
        for (at, cell) in row.children.iter().enumerate() {
            let Node::Block(cell) = cell else { continue };
            let tag = if header { "th" } else { "td" };
            match align.get(at).copied().unwrap_or_default() {
                Align::None => {
                    let _ = write!(out, "<{tag}>");
                }
                alignment => {
                    let _ = write!(out, "<{tag} align=\"{}\">", align_name(alignment));
                }
            }
            children(cell, theme, out);
            let _ = writeln!(out, "</{tag}>");
        }
        out.push_str("</tr>\n");
    }
    if let Some(open) = section {
        let _ = writeln!(out, "</{open}>");
    }
}

fn align_name(alignment: Align) -> &'static str {
    match alignment {
        Align::Left => "left",
        Align::Center => "center",
        Align::Right => "right",
        Align::None => "",
    }
}

fn wrapped(tag: &str, block_: &Block, theme: &mut dyn Blocks, out: &mut String) {
    let _ = writeln!(out, "<{tag}>");
    children(block_, theme, out);
    let _ = writeln!(out, "</{tag}>");
}

/// An opening tag carrying the block's ID, which is what a block-level comment
/// or a deep link points at (§7.16).
fn open(tag: &str, block: &Block) -> String {
    match &block.explicit_id {
        Some(id) => format!("<{tag} id=\"{}\">", escape(id)),
        None => format!("<{tag}>"),
    }
}

fn children(block_: &Block, theme: &mut dyn Blocks, out: &mut String) {
    for child in &block_.children {
        match child {
            Node::Block(child) => block(child, theme, out),
            Node::Inline(child) => inline(child, theme, out),
        }
    }
}

fn inline(node: &Inline, theme: &mut dyn Blocks, out: &mut String) {
    match node {
        Inline::Text(text) => out.push_str(&escape(text)),
        Inline::Emph(children) => tagged("em", children, theme, out),
        Inline::Strong(children) => tagged("strong", children, theme, out),
        Inline::Strike(children) => tagged("del", children, theme, out),
        Inline::Code(text) => {
            let _ = write!(out, "<code>{}</code>", escape(text));
        }
        Inline::Link {
            href,
            title,
            children,
            resolved,
        } => {
            let href = resolved
                .as_ref()
                .map_or(href.as_str(), |route| route.as_str());
            match title {
                Some(title) => {
                    let _ = write!(
                        out,
                        "<a href=\"{}\" title=\"{}\">",
                        escape(href),
                        escape(title)
                    );
                }
                None => {
                    let _ = write!(out, "<a href=\"{}\">", escape(href));
                }
            }
            for child in children {
                inline(child, theme, out);
            }
            out.push_str("</a>");
        }
        Inline::Image {
            src, alt, title, ..
        } => {
            let _ = write!(out, "<img src=\"{}\" alt=\"{}\"", escape(src), escape(alt));
            if let Some(title) = title {
                let _ = write!(out, " title=\"{}\"", escape(title));
            }
            out.push_str(" />");
        }
        Inline::HtmlInline(html) => out.push_str(html),
        Inline::FootnoteRef(label) => {
            let _ = write!(
                out,
                "<sup><a href=\"#fn-{0}\" id=\"fnref-{0}\">{0}</a></sup>",
                escape(label)
            );
        }
        Inline::SoftBreak => out.push('\n'),
        Inline::HardBreak => out.push_str("<br />\n"),
        Inline::Math(src) => {
            let _ = write!(out, "<span class=\"math\">{}</span>", escape(src));
        }
        Inline::InlineComponent {
            name,
            props,
            children,
        } => {
            let mut inner = String::new();
            for child in children {
                inline(child, theme, &mut inner);
            }
            let inst = ComponentInst {
                name: name.clone(),
                props: props.clone(),
                children: children.iter().cloned().map(Node::Inline).collect(),
                slots: liyasa_core::document::Slots::default(),
                id: liyasa_core::BlockId([0; 12]),
                origin: liyasa_core::Origin::default(),
            };
            match theme.component(&inst, &inner) {
                Ok(html) => out.push_str(&html),
                Err(_) => {
                    let _ = write!(out, "<span class=\"{}\">{inner}</span>", escape(name));
                }
            }
        }
        // A `{{ … }}` left unexpanded belongs to the editor's tree, not a page.
        Inline::TemplateInline { .. } => {}
    }
}

fn tagged(tag: &str, children: &[Inline], theme: &mut dyn Blocks, out: &mut String) {
    let _ = write!(out, "<{tag}>");
    for child in children {
        inline(child, theme, out);
    }
    let _ = write!(out, "</{tag}>");
}

fn code_of(block: &Block) -> String {
    let mut out = String::new();
    for child in &block.children {
        match child {
            Node::Inline(Inline::Text(text) | Inline::Code(text)) => out.push_str(text),
            Node::Inline(Inline::SoftBreak | Inline::HardBreak) => out.push('\n'),
            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod tests;
