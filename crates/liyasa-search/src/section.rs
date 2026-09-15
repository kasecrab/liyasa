//! Rendered AST into section documents (SRC-01).
//!
//! H2 and H3 start a document; H4 and below stay inside the one above them,
//! because a reader who lands on a fourth-level anchor has no context and the
//! index gains little from the extra granularity. Prose and code are separated
//! here, once, so both indexes weigh a fence the same way (SRC-03).

use liyasa_core::document::{Block, BlockKind, Document, Inline, Node, PropValue};

use crate::doc::{PageMeta, SectionDocument};

pub fn extract(document: &Document, meta: &PageMeta) -> Vec<SectionDocument> {
    let mut sections = vec![Open::lead(meta)];
    let mut parent_heading: Option<String> = None;

    for node in &document.root.children {
        if let Node::Block(block) = node
            && let BlockKind::Heading { level, anchor } = &block.kind
            && matches!(level, 2 | 3)
        {
            let title = inline_text(&block.children);
            let mut breadcrumb = meta.breadcrumb.clone();
            if *level == 3 {
                breadcrumb.extend(parent_heading.clone());
            } else {
                parent_heading = Some(title.clone());
            }
            sections.push(Open {
                anchor: anchor.clone(),
                section: title,
                breadcrumb,
                body: Text::default(),
                code: Text::default(),
            });
            continue;
        }
        let open = sections
            .last_mut()
            .expect("the lead section is never popped");
        collect(node, &mut open.body, &mut open.code);
    }

    let mut out = Vec::with_capacity(sections.len());
    for (n, open) in sections.into_iter().enumerate() {
        let lead = n == 0;
        if lead && open.body.is_empty() && open.code.is_empty() && meta.endpoint.is_none() {
            continue;
        }
        out.push(open.finish(meta, lead));
    }
    out
}

/// A section being filled. Separate from [`SectionDocument`] so the page's
/// facets are applied once, at the end, rather than copied per node.
struct Open {
    anchor: String,
    section: String,
    breadcrumb: Vec<String>,
    body: Text,
    code: Text,
}

impl Open {
    fn lead(meta: &PageMeta) -> Self {
        Self {
            anchor: String::new(),
            section: meta.title.clone(),
            breadcrumb: meta.breadcrumb.clone(),
            body: Text::default(),
            code: Text::default(),
        }
    }

    fn finish(self, meta: &PageMeta, lead: bool) -> SectionDocument {
        let mut body = self.body;
        let mut code = self.code;
        let mut keywords = meta.keywords.clone();

        if let Some(endpoint) = meta.endpoint.as_ref().filter(|_| lead) {
            let mut prose = Text::default();
            prose.push(&endpoint.summary);
            for parameter in &endpoint.parameters {
                prose.push(parameter);
            }
            prose.push(&body.finish());
            body = prose;
            let mut symbols = Text::default();
            symbols.push(&format!("{} {}", endpoint.method, endpoint.path));
            symbols.push(&code.finish());
            code = symbols;
            keywords.push(endpoint.method.clone());
            keywords.extend(endpoint.responses.iter().cloned());
        }

        SectionDocument {
            route: meta.route.clone(),
            anchor: self.anchor,
            title: meta.title.clone(),
            section: self.section,
            breadcrumb: self.breadcrumb,
            body: body.finish(),
            code: code.finish(),
            keywords,
            tab: meta.tab.clone(),
            version: meta.version.clone(),
            locale: meta.locale.clone(),
            kind: meta.kind,
            boost: meta.boost,
            groups: meta.groups.clone(),
            regions: meta.regions.clone(),
            updated: meta.updated,
        }
    }
}

/// An accumulator that inserts one space between pieces and nowhere else, so
/// two paragraphs never run their words together and a snippet offset means
/// what it says.
#[derive(Default)]
struct Text(String);

impl Text {
    fn push(&mut self, piece: &str) {
        let piece = piece.trim();
        if piece.is_empty() {
            return;
        }
        if !self.0.is_empty() {
            self.0.push(' ');
        }
        self.0.push_str(piece);
    }

    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    fn finish(self) -> String {
        self.0
    }
}

fn collect(node: &Node, body: &mut Text, code: &mut Text) {
    match node {
        Node::Block(block) => collect_block(block, body, code),
        Node::Inline(inline) => collect_inline(inline, body, code),
    }
}

fn collect_block(block: &Block, body: &mut Text, code: &mut Text) {
    match &block.kind {
        // A fence is code whatever it holds; `highlighted` is markup and the
        // original source is in the children.
        BlockKind::CodeBlock { .. } => {
            let mut fence = Text::default();
            let mut nested = Text::default();
            for child in &block.children {
                collect(child, &mut fence, &mut nested);
            }
            code.push(&fence.finish());
            code.push(&nested.finish());
        }
        // Raw HTML, unexpanded template markers, and rendered maths are markup
        // rather than content; none of them belong in a search snippet.
        BlockKind::HtmlBlock { .. } | BlockKind::Math { .. } | BlockKind::LogicMarker { .. } => {}
        BlockKind::Component { props, slots, .. } => {
            for value in props.0.values() {
                collect_prop(value, body);
            }
            for child in &block.children {
                collect(child, body, code);
            }
            for nodes in slots.0.values() {
                for child in nodes {
                    collect(child, body, code);
                }
            }
        }
        _ => {
            for child in &block.children {
                collect(child, body, code);
            }
        }
    }
}

fn collect_inline(inline: &Inline, body: &mut Text, code: &mut Text) {
    match inline {
        Inline::Text(value) => body.push(value),
        Inline::Code(value) => code.push(value),
        Inline::Emph(children) | Inline::Strong(children) | Inline::Strike(children) => {
            for child in children {
                collect_inline(child, body, code);
            }
        }
        Inline::Link { children, .. } => {
            for child in children {
                collect_inline(child, body, code);
            }
        }
        Inline::Image { alt, .. } => body.push(alt),
        Inline::InlineComponent {
            props, children, ..
        } => {
            for value in props.0.values() {
                collect_prop(value, body);
            }
            for child in children {
                collect_inline(child, body, code);
            }
        }
        Inline::SoftBreak | Inline::HardBreak => {}
        // `TemplateInline` is an unexpanded `{{ … }}`; SRC-12 keeps every
        // reader-facing expression out of every shared index.
        Inline::HtmlInline(_)
        | Inline::FootnoteRef(_)
        | Inline::Math(_)
        | Inline::TemplateInline { .. } => {}
    }
}

fn collect_prop(value: &PropValue, body: &mut Text) {
    match value {
        PropValue::Str(text) => body.push(text),
        PropValue::List(values) => {
            for value in values {
                collect_prop(value, body);
            }
        }
        // `Expr` is unevaluated; `Num` and `Bool` are settings, not prose.
        PropValue::Num(_) | PropValue::Bool(_) | PropValue::Expr(_) => {}
    }
}

fn inline_text(children: &[Node]) -> String {
    let mut body = Text::default();
    let mut code = Text::default();
    for child in children {
        collect(child, &mut body, &mut code);
    }
    let text = body.finish();
    if text.is_empty() { code.finish() } else { text }
}
