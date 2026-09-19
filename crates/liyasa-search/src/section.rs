//! Rendered AST into section documents (SRC-01).
//!
//! H2 and H3 start a document; H4 and below stay inside the one above them,
//! because a reader who lands on a fourth-level anchor has no context and the
//! index gains little from the extra granularity. Prose and code are separated
//! here, once, so both indexes weigh a fence the same way (SRC-03).

use liyasa_core::document::{Block, BlockKind, Document, Inline, Node, PropValue, Props};

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

/// Whether a component withholds its children from somebody.
///
/// The index is one artefact for every reader, so a block that is not shown to
/// everybody is indexed for nobody. `liyasa-components` applies these gates at
/// render time through `Shared`; `index_site` hands the whole Rendered AST to
/// the extractor instead, so the component gate never runs and this walk is
/// the only thing between a `:::visibility{groups=["admin"]}` block and a
/// snippet any anonymous reader can search.
///
/// Keyed on the prop names rather than the component name. `liyasa-components`
/// recognises `visibility` and `region` by name in its own ctx-free walk
/// (`text.rs`, TODO(rfc-0401)), which is exact for the two components that
/// exist today and silently admits the third one somebody adds. These names
/// are reserved for gating by the component schemas, so any component
/// declaring one withholds its children from someone by declaring it.
///
/// Over-excluding is the safe direction and the cost is small: a block gated
/// by `versions` or `locales` is not secret, but it is also not representable
/// in a section whose facets are the page's, so indexing it would let a v1
/// reader match v2 prose in a section labelled v1.
fn gates(props: &Props) -> bool {
    const GATES: [&str; 6] = ["groups", "regions", "locales", "versions", "only", "except"];
    GATES.iter().any(|name| match props.get(name) {
        Some(PropValue::List(items)) => !items.is_empty(),
        Some(PropValue::Str(text)) => !text.is_empty(),
        // An expression the build could not resolve is a gate whose answer is
        // unknown, and an unknown gate is one that did not hold (RFC 0401).
        Some(PropValue::Expr(_)) => true,
        _ => false,
    })
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
            // A gated block's contents belong to some readers and the index
            // belongs to all of them, so its children, its slots and its own
            // prop values are all withheld (SRC-12).
            if gates(props) {
                return;
            }
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
