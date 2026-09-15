//! A component registry for this crate's tests.
//!
//! The real registry is `liyasa-components`, which depends on this crate; the
//! parser therefore cannot use it and must not wait for it. These are the
//! shapes the corpus exercises and nothing else.

use liyasa_core::components::{
    Component, ComponentInst, ComponentRegistry, EditorBlock, MdCtx, PropDef, PropSchema, PropType,
    RenderCtx, RenderError, SlotDef,
};
use liyasa_core::document::Dep;
use liyasa_core::markdown::ComponentKind;

pub struct Fake {
    name: &'static str,
    aliases: &'static [&'static str],
    kind: ComponentKind,
    schema: PropSchema,
}

impl Component for Fake {
    fn name(&self) -> &'static str {
        self.name
    }

    fn aliases(&self) -> &'static [&'static str] {
        self.aliases
    }

    fn schema(&self) -> &PropSchema {
        &self.schema
    }

    fn kind(&self) -> ComponentKind {
        self.kind
    }

    fn render_html(&self, _: &ComponentInst, _: &mut RenderCtx) -> Result<(), RenderError> {
        Ok(())
    }

    fn render_markdown(&self, _: &ComponentInst, _: &mut MdCtx) -> Result<(), RenderError> {
        Ok(())
    }

    fn render_text(&self, _: &ComponentInst) -> String {
        String::new()
    }

    fn editor_block(&self) -> EditorBlock {
        EditorBlock {
            icon: "square".to_owned(),
            category: "test".to_owned(),
            form: Vec::new(),
            inline: self.kind == ComponentKind::Inline,
        }
    }

    fn deps(&self, _: &ComponentInst) -> Vec<Dep> {
        Vec::new()
    }
}

pub struct Registry(Vec<Fake>);

impl ComponentRegistry for Registry {
    fn get(&self, name: &str) -> Option<&dyn Component> {
        self.0
            .iter()
            .find(|c| c.name == name || c.aliases.contains(&name))
            .map(|c| c as &dyn Component)
    }

    fn names(&self) -> Vec<&str> {
        self.0.iter().map(|c| c.name).collect()
    }
}

fn prop(name: &'static str, ty: PropType, required: bool) -> PropDef {
    PropDef {
        name,
        ty,
        required,
        default: None,
        doc: "a test prop",
    }
}

fn slot(name: &'static str) -> SlotDef {
    SlotDef {
        name,
        required: false,
        doc: "a test slot",
    }
}

fn component(
    name: &'static str,
    aliases: &'static [&'static str],
    kind: ComponentKind,
    props: Vec<PropDef>,
    slots: Vec<SlotDef>,
) -> Fake {
    Fake {
        name,
        aliases,
        kind,
        schema: PropSchema { props, slots },
    }
}

pub fn registry() -> Registry {
    use ComponentKind::{Container, Inline, Leaf};
    Registry(vec![
        component(
            "card",
            &["Card"],
            Container,
            vec![
                prop("title", PropType::Str, false),
                prop("columns", PropType::Num, false),
                prop("open", PropType::Bool, false),
                prop("tags", PropType::List(Box::new(PropType::Str)), false),
                prop(
                    "variant",
                    PropType::Enum(vec!["wide".to_owned(), "narrow".to_owned()]),
                    false,
                ),
                prop("href", PropType::Route, false),
            ],
            vec![slot("header"), slot("footer")],
        ),
        component("note", &["Note"], Container, Vec::new(), Vec::new()),
        component("tip", &[], Container, Vec::new(), Vec::new()),
        component("important", &[], Container, Vec::new(), Vec::new()),
        component(
            "warning",
            &[],
            Container,
            vec![prop("title", PropType::Str, false)],
            Vec::new(),
        ),
        component("caution", &[], Container, Vec::new(), Vec::new()),
        component("callout", &[], Container, Vec::new(), Vec::new()),
        component("tabs", &["Tabs"], Container, Vec::new(), Vec::new()),
        component(
            "tab",
            &["Tab"],
            Container,
            vec![prop("title", PropType::Str, false)],
            Vec::new(),
        ),
        component(
            "image",
            &["Image"],
            Leaf,
            vec![
                prop("src", PropType::Asset, true),
                prop("alt", PropType::Str, false),
            ],
            Vec::new(),
        ),
        component("divider", &[], Leaf, Vec::new(), Vec::new()),
        component("kbd", &["Kbd"], Inline, Vec::new(), Vec::new()),
        component("sup", &[], Inline, Vec::new(), Vec::new()),
        component("sub", &[], Inline, Vec::new(), Vec::new()),
        component("mark", &[], Inline, Vec::new(), Vec::new()),
        component("underline", &[], Inline, Vec::new(), Vec::new()),
        component("insert", &[], Inline, Vec::new(), Vec::new()),
        component("spoiler", &[], Inline, Vec::new(), Vec::new()),
        component("subtext", &[], Inline, Vec::new(), Vec::new()),
        component("description-list", &[], Container, Vec::new(), Vec::new()),
        component("description-item", &[], Container, Vec::new(), Vec::new()),
        component("description-term", &[], Container, Vec::new(), Vec::new()),
        component(
            "description-details",
            &[],
            Container,
            Vec::new(),
            Vec::new(),
        ),
    ])
}

/// The registry plus the names `spec/markdown/` uses for illustration, so a
/// corpus case about fence lengths is not also a case about whether `:::a` is
/// a component anyone declared.
pub fn corpus_registry() -> Registry {
    use ComponentKind::{Container, Inline};
    let mut registry = registry();
    registry.0.extend([
        component("a", &[], Container, Vec::new(), Vec::new()),
        component("b", &[], Container, Vec::new(), Vec::new()),
        component("c", &[], Container, Vec::new(), Vec::new()),
        component("img", &[], ComponentKind::Leaf, Vec::new(), Vec::new()),
        component(
            "visibility",
            &[],
            Container,
            vec![prop(
                "groups",
                PropType::List(Box::new(PropType::Str)),
                false,
            )],
            Vec::new(),
        ),
        component(
            "badge",
            &[],
            Inline,
            vec![prop("color", PropType::Str, false)],
            Vec::new(),
        ),
        component("ref", &[], Inline, Vec::new(), Vec::new()),
    ]);
    registry
}

// ---- parsing a page in one call ----

use liyasa_core::document::{Block, BlockKind, Document, Inline, Node};
use liyasa_core::markdown::{Expanded, ExpansionRecord, HtmlMode, ParseOptions, SpanMap};

/// A fixed nonce: a test compares output, and a random one would make the
/// rewritten text differ between runs.
pub const NONCE: [u8; 16] = [0x5a; 16];

pub fn expanded(source: &str) -> Expanded {
    Expanded {
        text: source.to_owned(),
        map: SpanMap::default(),
        record: ExpansionRecord::default(),
    }
}

pub fn options() -> ParseOptions {
    ParseOptions {
        build_nonce: NONCE,
        ..ParseOptions::default()
    }
}

pub fn document(source: &str) -> Document {
    crate::ast::parse(&expanded(source), &registry(), &options())
}

pub fn document_with(source: &str, html: HtmlMode) -> Document {
    crate::ast::parse(
        &expanded(source),
        &registry(),
        &ParseOptions { html, ..options() },
    )
}

pub fn codes(document: &Document) -> Vec<&str> {
    document
        .diagnostics
        .iter()
        .map(|d| d.code.as_str())
        .collect()
}

/// Every block in the tree, in document order.
pub fn blocks(block: &Block) -> Vec<&Block> {
    let mut out = vec![block];
    for child in &block.children {
        if let Node::Block(child) = child {
            out.extend(blocks(child));
        }
    }
    out
}

/// Every component in the tree, by name.
pub fn components(block: &Block) -> Vec<&Block> {
    blocks(block)
        .into_iter()
        .filter(|b| matches!(b.kind, BlockKind::Component { .. }))
        .collect()
}

pub fn component_named<'a>(block: &'a Block, name: &str) -> &'a Block {
    components(block)
        .into_iter()
        .find(|b| matches!(&b.kind, BlockKind::Component { name: seen, .. } if seen == name))
        .unwrap_or_else(|| panic!("no `{name}` component in {block:#?}"))
}

/// Every inline in the tree, flattened.
pub fn inlines(block: &Block) -> Vec<&Inline> {
    let mut out = Vec::new();
    for child in &block.children {
        match child {
            Node::Block(child) => out.extend(inlines(child)),
            Node::Inline(inline) => collect_inline(inline, &mut out),
        }
    }
    out
}

fn collect_inline<'a>(inline: &'a Inline, out: &mut Vec<&'a Inline>) {
    out.push(inline);
    match inline {
        Inline::Emph(children)
        | Inline::Strong(children)
        | Inline::Strike(children)
        | Inline::Link { children, .. }
        | Inline::InlineComponent { children, .. } => {
            for child in children {
                collect_inline(child, out);
            }
        }
        _ => {}
    }
}

/// The raw HTML left in the tree, joined.
pub fn html_of(block: &Block) -> String {
    let mut out: Vec<String> = blocks(block)
        .into_iter()
        .filter_map(|b| match &b.kind {
            BlockKind::HtmlBlock { html } => Some(html.clone()),
            _ => None,
        })
        .collect();
    out.extend(inlines(block).into_iter().filter_map(|i| match i {
        Inline::HtmlInline(html) => Some(html.clone()),
        _ => None,
    }));
    out.join("")
}
