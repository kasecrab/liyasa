//! Rendering a node tree, so components can be tested before `liyasa-markdown`
//! exists.
//!
//! WP-03 owns the real AST walk; this one covers the [`BlockKind`] and
//! [`Inline`] variants a component's children can hold and dispatches nested
//! components back through the registry. It is the [`Children`] implementation
//! the golden tests run against, and a working default for any caller that has
//! a tree and no walker.

use liyasa_core::components::RenderError;
use liyasa_core::document::{Align, Block, BlockKind, Inline, Node};

use crate::html::Html;
use crate::md::Markdown;
use crate::registry::Registry;
use crate::render::{Children, HtmlCtx, MarkdownCtx, Shared};

pub struct Reference<'r> {
    registry: Option<&'r Registry>,
}

impl Default for Reference<'_> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'r> Reference<'r> {
    pub fn new() -> Self {
        Self { registry: None }
    }

    pub fn with(registry: &'r Registry) -> Self {
        Self {
            registry: Some(registry),
        }
    }

    fn component_html(&self, block: &Block, out: &mut Html) -> Result<(), RenderError> {
        let Some(inst) = crate::nodes::as_component(&Node::Block(block.clone())) else {
            return Ok(());
        };
        let Some(component) = self.registry.and_then(|r| r.resolve(&inst.name)) else {
            // An unknown component is reported by validation, not by the
            // renderer; here its children are all that can be salvaged.
            return self.html(&block.children, out);
        };
        // The component writes into the caller's buffer, not into a fragment:
        // a nested component keeps the surrounding element stack.
        let mut ctx = HtmlCtx::with(Shared::new(self));
        ctx.out = std::mem::take(out);
        let result = component.html(&inst, &mut ctx);
        *out = ctx.out;
        result
    }

    fn component_markdown(&self, block: &Block, out: &mut Markdown) -> Result<(), RenderError> {
        let Some(inst) = crate::nodes::as_component(&Node::Block(block.clone())) else {
            return Ok(());
        };
        let Some(component) = self.registry.and_then(|r| r.resolve(&inst.name)) else {
            return self.markdown(&block.children, out);
        };
        // Written into the caller's buffer so the line prefix and a pending
        // list marker survive: a card inside a card group is one list item,
        // not a document of its own.
        let mut ctx = MarkdownCtx::with(Shared::new(self));
        ctx.out = std::mem::take(out);
        let result = component.markdown(&inst, &mut ctx);
        *out = ctx.out;
        result
    }

    fn block_html(&self, block: &Block, out: &mut Html) -> Result<(), RenderError> {
        match &block.kind {
            BlockKind::Document => self.html(&block.children, out)?,
            BlockKind::Heading { level, anchor } => {
                let tag = heading_tag(*level);
                out.open(tag).attr("id", anchor);
                self.html(&block.children, out)?;
                out.close();
            }
            BlockKind::Paragraph => {
                out.open("p");
                self.html(&block.children, out)?;
                out.close();
            }
            BlockKind::List { ordered, start, .. } => {
                if *ordered {
                    out.open("ol");
                    if *start != 1 {
                        out.attr("start", &start.to_string());
                    }
                } else {
                    out.open("ul");
                }
                self.html(&block.children, out)?;
                out.close();
            }
            BlockKind::ListItem { checked } => {
                out.open("li");
                if let Some(checked) = checked {
                    out.attr("class", "ly-task");
                    out.open("input")
                        .attr("type", "checkbox")
                        .flag_if("checked", *checked)
                        .flag("disabled");
                }
                self.html(&block.children, out)?;
                out.close();
            }
            BlockKind::BlockQuote => {
                out.open("blockquote");
                self.html(&block.children, out)?;
                out.close();
            }
            BlockKind::CodeBlock {
                lang,
                attrs,
                highlighted,
            } => {
                let body =
                    raw_code(&block.children).unwrap_or_else(|| crate::text::of(&block.children));
                if lang.as_deref() == Some("mermaid") {
                    crate::fence::render_mermaid(out, &body, attrs);
                } else {
                    let options = crate::fence::CodeOptions::read(attrs);
                    crate::fence::render_html(
                        out,
                        lang.as_deref(),
                        &body,
                        &options,
                        highlighted.as_deref(),
                    );
                }
            }
            BlockKind::HtmlBlock { html } => {
                out.raw(html);
            }
            BlockKind::Table { align } => {
                out.open("table");
                self.table_html(block, align, out)?;
                out.close();
            }
            BlockKind::TableRow { .. } | BlockKind::TableCell => {
                // Reached only when a row or a cell is rendered on its own.
                self.html(&block.children, out)?;
            }
            BlockKind::ThematicBreak => {
                out.open("hr");
            }
            BlockKind::FootnoteDefinition { label } => {
                out.open("div")
                    .attr("class", "ly-footnote")
                    .attr("id", &format!("fn-{label}"));
                self.html(&block.children, out)?;
                out.close();
            }
            BlockKind::Math { display, src } => {
                out.open(if *display { "div" } else { "span" })
                    .attr("class", "ly-math")
                    .flag_if("data-display", *display)
                    .text(src)
                    .close();
            }
            BlockKind::Component { .. } => self.component_html(block, out)?,
            BlockKind::LogicMarker { .. } => {}
        }
        Ok(())
    }

    fn table_html(
        &self,
        block: &Block,
        align: &[Align],
        out: &mut Html,
    ) -> Result<(), RenderError> {
        let mut in_body = false;
        for child in &block.children {
            let Node::Block(row) = child else { continue };
            let BlockKind::TableRow { header } = row.kind else {
                continue;
            };
            if header {
                out.open("thead");
            } else if !in_body {
                in_body = true;
                out.open("tbody");
            }
            out.open("tr");
            for (at, cell) in row.children.iter().enumerate() {
                let Node::Block(cell_block) = cell else {
                    continue;
                };
                out.open(if header { "th" } else { "td" });
                if let Some(style) = align.get(at).and_then(align_style) {
                    out.attr("style", style);
                }
                self.html(&cell_block.children, out)?;
                out.close();
            }
            out.close();
            if header {
                out.close();
            }
        }
        if in_body {
            out.close();
        }
        Ok(())
    }

    fn block_markdown(&self, block: &Block, out: &mut Markdown) -> Result<(), RenderError> {
        match &block.kind {
            BlockKind::Document => self.markdown(&block.children, out)?,
            BlockKind::Heading { level, .. } => {
                out.heading(*level, &inline_text(&block.children));
            }
            BlockKind::Paragraph => {
                out.block();
                self.markdown(&block.children, out)?;
                out.end_line();
            }
            BlockKind::List { ordered, start, .. } => {
                out.block();
                let mut number = *start;
                for child in &block.children {
                    let Node::Block(item) = child else { continue };
                    let marker = if *ordered {
                        format!("{number}. ")
                    } else {
                        "- ".to_owned()
                    };
                    let mut error = Ok(());
                    out.item(&marker, |md| error = self.markdown(&item.children, md));
                    error?;
                    number += 1;
                }
            }
            BlockKind::ListItem { .. } => self.markdown(&block.children, out)?,
            BlockKind::BlockQuote => {
                let mut error = Ok(());
                out.quote(|md| error = self.markdown(&block.children, md));
                error?;
            }
            BlockKind::CodeBlock { lang, attrs, .. } => {
                let options = crate::fence::CodeOptions::read(attrs);
                let body = raw_code(&block.children).unwrap_or_default();
                crate::fence::render_markdown(out, lang.as_deref(), &body, &options);
            }
            BlockKind::HtmlBlock { html } => {
                out.block();
                out.write(html.trim_end());
                out.end_line();
            }
            BlockKind::Table { .. } => self.table_markdown(block, out)?,
            BlockKind::TableRow { .. } | BlockKind::TableCell => {
                self.markdown(&block.children, out)?;
            }
            BlockKind::ThematicBreak => {
                out.thematic_break();
            }
            BlockKind::FootnoteDefinition { label } => {
                out.block();
                out.write(&format!("[^{label}]: "));
                self.markdown(&block.children, out)?;
                out.end_line();
            }
            BlockKind::Math { display, src } => {
                if *display {
                    out.block();
                    out.line(&format!("$${src}$$"));
                } else {
                    out.write(&format!("${src}$"));
                }
            }
            BlockKind::Component { .. } => self.component_markdown(block, out)?,
            BlockKind::LogicMarker { .. } => {}
        }
        Ok(())
    }

    fn table_markdown(&self, block: &Block, out: &mut Markdown) -> Result<(), RenderError> {
        let mut header: Vec<String> = Vec::new();
        let mut rows: Vec<Vec<String>> = Vec::new();
        for child in &block.children {
            let Node::Block(row) = child else { continue };
            let BlockKind::TableRow { header: is_header } = row.kind else {
                continue;
            };
            let cells: Vec<String> = row
                .children
                .iter()
                .map(|cell| match cell {
                    Node::Block(cell) => inline_text(&cell.children),
                    Node::Inline(inline) => crate::text::of(&[Node::Inline(inline.clone())]),
                })
                .collect();
            if is_header && header.is_empty() {
                header = cells;
            } else {
                rows.push(cells);
            }
        }
        out.table(&header, &rows);
        Ok(())
    }

    fn inline_markdown(&self, inline: &Inline, out: &mut Markdown) -> Result<(), RenderError> {
        match inline {
            Inline::Text(text) => {
                out.write(&crate::md::escape_inline(text));
            }
            Inline::Code(text) => {
                out.write(&crate::md::code_span(text));
            }
            Inline::Emph(children) => self.wrap_markdown("*", children, out)?,
            Inline::Strong(children) => self.wrap_markdown("**", children, out)?,
            Inline::Strike(children) => self.wrap_markdown("~~", children, out)?,
            Inline::Link { href, children, .. } => {
                out.write("[");
                self.inlines_markdown(children, out)?;
                out.write(&format!("]({})", crate::md::escape_url(href)));
            }
            Inline::Image { src, alt, .. } => {
                out.write(&format!(
                    "![{}]({})",
                    crate::md::escape_inline(alt),
                    crate::md::escape_url(src)
                ));
            }
            Inline::HtmlInline(html) => {
                out.write(html);
            }
            Inline::FootnoteRef(label) => {
                out.write(&format!("[^{label}]"));
            }
            Inline::SoftBreak => {
                out.end_line();
            }
            Inline::HardBreak => {
                out.write("  ");
                out.end_line();
            }
            Inline::InlineComponent {
                name,
                props,
                children,
            } => {
                let inst = crate::inst::new(name)
                    .props(props.clone())
                    .children(children.iter().cloned().map(Node::Inline))
                    .build();
                match self.registry.and_then(|r| r.resolve(name)) {
                    Some(component) => {
                        let mut ctx = MarkdownCtx::with(Shared::new(self));
                        ctx.out = std::mem::take(out);
                        let result = component.markdown(&inst, &mut ctx);
                        *out = ctx.out;
                        result?;
                    }
                    None => self.inlines_markdown(children, out)?,
                }
            }
            Inline::Math(src) => {
                out.write(&format!("${src}$"));
            }
            Inline::TemplateInline { expr, .. } => {
                out.write(&format!("{{{{{expr}}}}}"));
            }
        }
        Ok(())
    }

    fn wrap_markdown(
        &self,
        marker: &str,
        children: &[Inline],
        out: &mut Markdown,
    ) -> Result<(), RenderError> {
        out.write(marker);
        self.inlines_markdown(children, out)?;
        out.write(marker);
        Ok(())
    }

    fn inlines_markdown(&self, children: &[Inline], out: &mut Markdown) -> Result<(), RenderError> {
        for child in children {
            self.inline_markdown(child, out)?;
        }
        Ok(())
    }

    fn inline_html(&self, inline: &Inline, out: &mut Html) -> Result<(), RenderError> {
        match inline {
            Inline::Text(text) => {
                out.text(text);
            }
            Inline::Code(text) => {
                out.open("code").text(text).close();
            }
            Inline::Emph(children) => self.wrap_html("em", children, out)?,
            Inline::Strong(children) => self.wrap_html("strong", children, out)?,
            Inline::Strike(children) => self.wrap_html("del", children, out)?,
            Inline::Link {
                href,
                title,
                children,
                ..
            } => {
                out.open("a");
                if crate::props::is_safe_url(href) {
                    out.attr("href", href);
                }
                out.attr_if("title", title.as_deref());
                self.inlines_html(children, out)?;
                out.close();
            }
            Inline::Image {
                src, alt, title, ..
            } => {
                out.open("img");
                if crate::props::is_safe_url(src) {
                    out.attr("src", src);
                }
                out.attr("alt", alt).attr_if("title", title.as_deref());
            }
            Inline::HtmlInline(html) => {
                out.raw(html);
            }
            Inline::FootnoteRef(label) => {
                out.open("sup")
                    .open("a")
                    .attr("href", &format!("#fn-{label}"))
                    .text(label)
                    .close()
                    .close();
            }
            Inline::SoftBreak => {
                out.text("\n");
            }
            Inline::HardBreak => {
                out.open("br");
            }
            Inline::InlineComponent {
                name,
                props,
                children,
            } => {
                let inst = crate::inst::new(name)
                    .props(props.clone())
                    .children(children.iter().cloned().map(Node::Inline))
                    .build();
                match self.registry.and_then(|r| r.resolve(name)) {
                    Some(component) => {
                        let mut ctx = HtmlCtx::with(Shared::new(self));
                        ctx.out = std::mem::take(out);
                        let result = component.html(&inst, &mut ctx);
                        *out = ctx.out;
                        result?;
                    }
                    None => self.inlines_html(children, out)?,
                }
            }
            Inline::Math(src) => {
                out.open("span").attr("class", "ly-math").text(src).close();
            }
            Inline::TemplateInline { expr, .. } => {
                out.open("span")
                    .attr("class", "ly-template")
                    .text(expr)
                    .close();
            }
        }
        Ok(())
    }

    fn wrap_html(
        &self,
        tag: &'static str,
        children: &[Inline],
        out: &mut Html,
    ) -> Result<(), RenderError> {
        out.open(tag);
        self.inlines_html(children, out)?;
        out.close();
        Ok(())
    }

    fn inlines_html(&self, children: &[Inline], out: &mut Html) -> Result<(), RenderError> {
        for child in children {
            self.inline_html(child, out)?;
        }
        Ok(())
    }
}

impl Children for Reference<'_> {
    fn html(&self, nodes: &[Node], out: &mut Html) -> Result<(), RenderError> {
        for node in nodes {
            match node {
                Node::Block(block) => self.block_html(block, out)?,
                Node::Inline(inline) => self.inline_html(inline, out)?,
            }
        }
        Ok(())
    }

    fn markdown(&self, nodes: &[Node], out: &mut Markdown) -> Result<(), RenderError> {
        let mut at = 0;
        while at < nodes.len() {
            // RX-61: a run of API fields is one table, so it is recognized
            // here rather than by each field, which cannot see its siblings.
            let run = field_run(&nodes[at..]);
            if run > 1 {
                let fields: Vec<_> = nodes[at..at + run]
                    .iter()
                    .filter_map(crate::nodes::as_component)
                    .collect();
                crate::components::api::table_markdown(&fields, out, self);
                at += run;
                continue;
            }
            match &nodes[at] {
                Node::Block(block) => self.block_markdown(block, out)?,
                Node::Inline(inline) => self.inline_markdown(inline, out)?,
            }
            at += 1;
        }
        Ok(())
    }
}

/// How many nodes from the front are API field components.
fn field_run(nodes: &[Node]) -> usize {
    nodes
        .iter()
        .take_while(|node| {
            crate::nodes::as_component(node).is_some_and(|inst| {
                crate::components::api::FIELD_NAMES.contains(&inst.name.as_str())
            })
        })
        .count()
}

/// A code block's body, which is one text inline and never marked up.
fn raw_code(children: &[Node]) -> Option<String> {
    let mut body = String::new();
    for child in children {
        match child {
            Node::Inline(Inline::Text(text)) => body.push_str(text),
            _ => return None,
        }
    }
    Some(body)
}

fn inline_text(children: &[Node]) -> String {
    let reference = Reference::new();
    let mut out = Markdown::new();
    let _ = reference.markdown(children, &mut out);
    out.finish().trim().replace('\n', " ")
}

fn heading_tag(level: u8) -> &'static str {
    match level {
        1 => "h1",
        2 => "h2",
        3 => "h3",
        4 => "h4",
        5 => "h5",
        _ => "h6",
    }
}

fn align_style(align: &Align) -> Option<&'static str> {
    match align {
        Align::None => None,
        Align::Left => Some("text-align: left"),
        Align::Center => Some("text-align: center"),
        Align::Right => Some("text-align: right"),
    }
}
