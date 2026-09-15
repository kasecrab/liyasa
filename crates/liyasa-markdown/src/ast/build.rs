//! comrak's tree turned into the Rendered AST of §7.16.
//!
//! The walk is deliberately dumb: it maps nodes, records origins, and leaves
//! every decision that needs page-wide context — anchors, block identity, slot
//! placement, whether a component name exists — to the passes that run after
//! it. Nothing here needs to look at a sibling.

use std::collections::BTreeMap;

use comrak::nodes::{AstNode, ListType, NodeValue};
use liyasa_core::document::{Align, Block, BlockKind, Inline, Node, Props};
use liyasa_core::ids::BlockId;
use liyasa_core::markdown::{ComponentKind, DirectiveTable, ParseOptions};
use liyasa_core::{Diagnostic, Diagnostics, Span};

use super::{fence, pos};
use crate::directives::{info, rewrite, tag};

/// The identity every block carries until the identity pass runs.
pub const UNASSIGNED: BlockId = BlockId([0; 12]);

/// How deep a page may nest before the parser stops walking.
///
/// comrak's own parser is iterative and will happily hand back twenty thousand
/// nested blockquotes; every pass over the tree after it recurses with the
/// nesting, and a stack overflow aborts the process rather than failing the
/// page. The reference CommonMark implementation draws the same line, and no
/// document a person wrote comes close to it.
pub const MAX_NESTING: usize = 128;

pub struct Builder<'a> {
    pub positions: pos::Positions<'a>,
    pub table: &'a DirectiveTable,
    /// The table row for each container open, by its 1-based line.
    pub containers: &'a BTreeMap<u32, usize>,
    pub opts: &'a ParseOptions,
    /// The rewritten text by line, for the one question comrak's tree cannot
    /// answer: whether a container was closed or merely ran out of input.
    pub lines: Vec<&'a str>,
    pub diagnostics: Diagnostics,
    /// Set once the nesting limit has been reported.
    pub deep: bool,
    /// How each component was written, by the span it occupies. A component
    /// block cannot tell you on its own whether it was `:::card` with an empty
    /// body or `::card`, and `E0317` is exactly that difference.
    pub written: BTreeMap<Span, ComponentKind>,
}

impl Builder<'_> {
    pub fn document<'a>(&mut self, root: &'a AstNode<'a>) -> Block {
        let mut block = self
            .block(root, 0)
            .unwrap_or_else(|| self.empty(BlockKind::Document));
        block.kind = BlockKind::Document;
        block
    }

    fn empty(&self, kind: BlockKind) -> Block {
        Block {
            id: UNASSIGNED,
            explicit_id: None,
            kind,
            origin: liyasa_core::Origin::default(),
            children: Vec::new(),
        }
    }

    fn block<'a>(&mut self, node: &'a AstNode<'a>, depth: usize) -> Option<Block> {
        let data = node.data.borrow();
        if depth > MAX_NESTING {
            self.too_deep(self.positions.span(data.sourcepos));
            return None;
        }
        let span = self.positions.span(data.sourcepos);
        let kind = match &data.value {
            NodeValue::Document => BlockKind::Document,
            // `$$…$$` alone in a paragraph is display math, and comrak reports
            // it as an inline node carrying a flag `Inline::Math` has no room
            // for. The block form has the room, and a display equation is a
            // block to every reader anyway.
            NodeValue::Paragraph => match display_math(node) {
                Some(src) => BlockKind::Math { display: true, src },
                None => BlockKind::Paragraph,
            },
            NodeValue::BlockQuote | NodeValue::MultilineBlockQuote(_) => BlockKind::BlockQuote,
            NodeValue::Heading(heading) => BlockKind::Heading {
                level: heading.level,
                anchor: String::new(),
            },
            NodeValue::List(list) => BlockKind::List {
                ordered: list.list_type == ListType::Ordered,
                start: list.start as u32,
                tight: list.tight,
            },
            NodeValue::Item(_) => BlockKind::ListItem { checked: None },
            NodeValue::TaskItem(item) => BlockKind::ListItem {
                checked: Some(item.symbol.is_some()),
            },
            NodeValue::CodeBlock(code) => {
                let parsed = fence::parse(&code.info);
                // comrak keeps a fence's body in the node rather than in
                // children; the Rendered AST has no such field, so the body is
                // the block's one text child.
                let literal = code.literal.clone();
                // The info string starts after the fence on the opening line.
                self.diagnostics
                    .extend(parsed.diagnostics(self.positions.source, span.start));
                return Some(Block {
                    id: UNASSIGNED,
                    explicit_id: None,
                    kind: BlockKind::CodeBlock {
                        lang: parsed.info.lang,
                        attrs: parsed.info.attrs,
                        // CM-38 highlights at build time, after the grammar set
                        // is resolved; the parser records what to highlight.
                        highlighted: None,
                    },
                    origin: self.positions.origin(span),
                    children: vec![Node::Inline(Inline::Text(literal))],
                });
            }
            NodeValue::HtmlBlock(html) => return Some(self.html_block(html, span)),
            NodeValue::ThematicBreak => BlockKind::ThematicBreak,
            NodeValue::FootnoteDefinition(footnote) => BlockKind::FootnoteDefinition {
                label: footnote.name.clone(),
            },
            NodeValue::Table(table) => BlockKind::Table {
                align: table.alignments.iter().copied().map(align).collect(),
            },
            NodeValue::TableRow(header) => BlockKind::TableRow { header: *header },
            NodeValue::TableCell => BlockKind::TableCell,
            NodeValue::Math(math) => BlockKind::Math {
                display: math.display_math,
                src: math.literal.clone(),
            },
            NodeValue::BlockDirective(directive) => {
                // The props of a directive that carried any were taken out of
                // the line before comrak saw it, and are matched back by line.
                let (name, props) = match self
                    .containers
                    .get(&(data.sourcepos.start.line as u32))
                    .and_then(|id| self.table.get(*id))
                {
                    Some(row) => (row.name.clone(), row.props.clone()),
                    None => {
                        let info = info::parse(&directive.info);
                        self.diagnostics.extend(info.diagnostics(
                            self.positions.source,
                            span.start + directive.fence_length as u32,
                        ));
                        (info.name, info.props)
                    }
                };
                // comrak reports a `:::` with nothing to close as a directive
                // with an empty info string, which is §7.5.1's `E0311`.
                if name.is_empty() && props.is_empty() {
                    self.diagnostics.push(
                        Diagnostic::new(
                            liyasa_core::diagnostics::code::E0311,
                            "this `:::` closes a container directive that was never opened",
                        )
                        .at(span),
                    );
                    return None;
                }
                // comrak closes an open container at end of input rather than
                // reporting it, so §7.5.1's `E0310` is a check over the tree.
                if !self.closed(node, directive.fence_length) {
                    self.diagnostics.push(
                        Diagnostic::new(
                            liyasa_core::diagnostics::code::E0310,
                            format!("`:::{name}` is never closed"),
                        )
                        .at(span)
                        .help(format!(
                            "add a closing `{}` line",
                            ":".repeat(directive.fence_length)
                        )),
                    );
                }
                self.written.insert(span, ComponentKind::Container);
                BlockKind::Component {
                    name,
                    props,
                    slots: liyasa_core::document::Slots::default(),
                }
            }
            // CM-34: a GitHub alert is a callout, so `> [!NOTE]` and
            // `:::note` reach the theme as the same component.
            NodeValue::Alert(alert) => {
                self.written.insert(span, ComponentKind::Container);
                BlockKind::Component {
                    name: alert_name(alert.alert_type).to_owned(),
                    props: alert_props(alert.title.as_deref()),
                    slots: liyasa_core::document::Slots::default(),
                }
            }
            // CM-40: the contract has no description-list kind, so the
            // extension point carries it.
            NodeValue::DescriptionList => self.described("description-list", span),
            NodeValue::DescriptionItem(_) => self.described("description-item", span),
            NodeValue::DescriptionTerm => self.described("description-term", span),
            NodeValue::DescriptionDetails => self.described("description-details", span),
            // Front matter belongs to the Source Document, not to this tree.
            NodeValue::FrontMatter(_) => return None,
            _ => return None,
        };

        Some(Block {
            id: UNASSIGNED,
            explicit_id: None,
            kind,
            origin: self.positions.origin(span),
            children: self.children(node, depth),
        })
    }

    /// Whether a container directive's last line is its own closing fence.
    ///
    /// A fence that a nested directive already closed does not count: comrak
    /// gives a parent and its last child the same end line either way.
    fn closed<'a>(&self, node: &'a AstNode<'a>, fence_length: usize) -> bool {
        let end = node.data.borrow().sourcepos.end.line;
        let Some(line) = self.lines.get(end.saturating_sub(1)) else {
            return false;
        };
        let fence = crate::directives::mask::content_of(line).trim();
        if fence.is_empty() || !fence.chars().all(|c| c == ':') || fence.len() < fence_length {
            return false;
        }
        !node.descendants().skip(1).any(|child| {
            let data = child.data.borrow();
            matches!(data.value, NodeValue::BlockDirective(_)) && data.sourcepos.end.line == end
        })
    }

    fn described(&mut self, name: &str, span: Span) -> BlockKind {
        self.written.insert(span, ComponentKind::Container);
        BlockKind::Component {
            name: name.to_owned(),
            props: Props::default(),
            slots: liyasa_core::document::Slots::default(),
        }
    }

    /// An HTML block is a leaf directive's marker, a component tag, or HTML.
    fn html_block(&mut self, html: &comrak::nodes::NodeHtmlBlock, span: Span) -> Block {
        let origin = self.positions.origin(span);
        if let Some(id) = rewrite::marker_id(&html.literal, self.opts.build_nonce)
            && let Some(row) = self.table.get(id)
        {
            self.written.insert(span, ComponentKind::Leaf);
            return Block {
                id: UNASSIGNED,
                explicit_id: None,
                kind: BlockKind::Component {
                    name: row.name.clone(),
                    props: row.props.clone(),
                    slots: liyasa_core::document::Slots::default(),
                },
                origin,
                children: Vec::new(),
            };
        }
        Block {
            id: UNASSIGNED,
            explicit_id: None,
            kind: BlockKind::HtmlBlock {
                html: html.literal.clone(),
            },
            origin,
            children: Vec::new(),
        }
    }

    fn children<'a>(&mut self, node: &'a AstNode<'a>, depth: usize) -> Vec<Node> {
        let mut out = Vec::new();
        for child in node.children() {
            if is_inline(&child.data.borrow().value) {
                let inlines = self.inlines(node, depth);
                out.extend(inlines.into_iter().map(Node::Inline));
                return tag::nest_blocks(out);
            }
            if let Some(block) = self.block(child, depth + 1) {
                out.push(Node::Block(block));
            }
        }
        tag::nest_blocks(out)
    }

    fn inlines<'a>(&mut self, node: &'a AstNode<'a>, depth: usize) -> Vec<Inline> {
        let out: Vec<Inline> = node
            .children()
            .filter_map(|c| self.inline(c, depth + 1))
            .collect();
        crate::directives::inline::scan(tag::nest_inlines(out))
    }

    fn inline<'a>(&mut self, node: &'a AstNode<'a>, depth: usize) -> Option<Inline> {
        let data = node.data.borrow();
        if depth > MAX_NESTING {
            self.too_deep(self.positions.span(data.sourcepos));
            return None;
        }
        Some(match &data.value {
            NodeValue::Text(text) => Inline::Text(text.to_string()),
            NodeValue::Emph => Inline::Emph(self.inlines(node, depth)),
            NodeValue::Strong => Inline::Strong(self.inlines(node, depth)),
            NodeValue::Strikethrough => Inline::Strike(self.inlines(node, depth)),
            NodeValue::Code(code) => Inline::Code(code.literal.clone()),
            NodeValue::HtmlInline(html) => Inline::HtmlInline(html.clone()),
            NodeValue::SoftBreak => Inline::SoftBreak,
            NodeValue::LineBreak => Inline::HardBreak,
            NodeValue::Link(link) => Inline::Link {
                href: link.url.clone(),
                title: (!link.title.is_empty()).then(|| link.title.clone()),
                children: self.inlines(node, depth),
                resolved: None,
            },
            NodeValue::WikiLink(link) => Inline::Link {
                href: link.url.clone(),
                title: None,
                children: self.inlines(node, depth),
                resolved: None,
            },
            NodeValue::Image(image) => Inline::Image {
                src: image.url.clone(),
                alt: plain(&self.inlines(node, depth)),
                title: (!image.title.is_empty()).then(|| image.title.clone()),
                dark: None,
            },
            NodeValue::FootnoteReference(footnote) => Inline::FootnoteRef(footnote.name.clone()),
            NodeValue::Math(math) => Inline::Math(math.literal.clone()),
            // CM-41: comrak resolves the shortcode against the `emojis` table.
            NodeValue::ShortCode(shortcode) => Inline::Text(shortcode.emoji.to_owned()),
            NodeValue::EscapedTag(text) => Inline::Text((*text).to_owned()),
            // A backslash escape is the character it escaped.
            NodeValue::Escaped => return None,
            NodeValue::Raw(text) => Inline::HtmlInline(text.clone()),
            // The remaining inline extensions have no variant of their own in
            // the frozen `Inline`, so they reach the theme as components.
            NodeValue::Superscript => self.wrapped("sup", node, depth),
            NodeValue::Subscript => self.wrapped("sub", node, depth),
            NodeValue::Underline => self.wrapped("underline", node, depth),
            NodeValue::Highlight => self.wrapped("mark", node, depth),
            NodeValue::Insert => self.wrapped("insert", node, depth),
            NodeValue::SpoileredText => self.wrapped("spoiler", node, depth),
            NodeValue::Subtext => self.wrapped("subtext", node, depth),
            _ => return None,
        })
    }

    fn wrapped<'a>(&mut self, name: &str, node: &'a AstNode<'a>, depth: usize) -> Inline {
        Inline::InlineComponent {
            name: name.to_owned(),
            props: Props::default(),
            children: self.inlines(node, depth),
        }
    }

    /// Reported once: a page that nests past the limit would otherwise report
    /// one error per level all the way down.
    fn too_deep(&mut self, span: Span) {
        if self.deep {
            return;
        }
        self.deep = true;
        self.diagnostics.push(
            Diagnostic::new(
                liyasa_core::diagnostics::code::E0322,
                format!("this page nests more than {MAX_NESTING} levels deep"),
            )
            .at(span)
            .help("the content below that point was not parsed"),
        );
    }
}

/// The LaTeX of a paragraph that is nothing but one display equation.
fn display_math<'a>(node: &'a AstNode<'a>) -> Option<String> {
    let mut children = node.children();
    let only = children.next()?;
    if children.next().is_some() {
        return None;
    }
    match &only.data.borrow().value {
        NodeValue::Math(math) if math.display_math => Some(math.literal.clone()),
        _ => None,
    }
}

fn alert_name(kind: comrak::nodes::AlertType) -> &'static str {
    use comrak::nodes::AlertType;
    match kind {
        AlertType::Note => "note",
        AlertType::Tip => "tip",
        AlertType::Important => "important",
        AlertType::Warning => "warning",
        AlertType::Caution => "caution",
    }
}

fn alert_props(title: Option<&str>) -> Props {
    let mut props = Props::default();
    if let Some(title) = title {
        props.0.insert(
            "title".to_owned(),
            liyasa_core::document::PropValue::Str(title.to_owned()),
        );
    }
    props
}

fn align(alignment: comrak::nodes::TableAlignment) -> Align {
    use comrak::nodes::TableAlignment;
    match alignment {
        TableAlignment::None => Align::None,
        TableAlignment::Left => Align::Left,
        TableAlignment::Center => Align::Center,
        TableAlignment::Right => Align::Right,
    }
}

fn is_inline(value: &NodeValue) -> bool {
    !value.block()
}

/// The text of an inline run, for an image's alt text and for block identity.
pub fn plain(children: &[Inline]) -> String {
    let mut out = String::new();
    for child in children {
        match child {
            Inline::Text(text) | Inline::Code(text) | Inline::Math(text) => out.push_str(text),
            Inline::Emph(inner)
            | Inline::Strong(inner)
            | Inline::Strike(inner)
            | Inline::Link {
                children: inner, ..
            }
            | Inline::InlineComponent {
                children: inner, ..
            } => out.push_str(&plain(inner)),
            Inline::Image { alt, .. } => out.push_str(alt),
            Inline::SoftBreak | Inline::HardBreak => out.push(' '),
            Inline::HtmlInline(_) | Inline::FootnoteRef(_) | Inline::TemplateInline { .. } => {}
        }
    }
    out
}

/// Whitespace runs collapsed, so a block's identity does not move when its
/// source is rewrapped.
pub fn normalize(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}
