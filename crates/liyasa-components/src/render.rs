//! The rendering surface components actually use.
//!
//! §34.9's `Component::render_html` takes a `&mut RenderCtx` that has no fields
//! and no constructor, so it can neither be called nor written to from outside
//! `liyasa-core`. Until that is resolved (`plan/rfcs/0004-component-render-sinks.md`)
//! the sinks live here: a component writes into [`HtmlCtx`] or [`MarkdownCtx`],
//! and the frozen methods are adapters.

use liyasa_core::components::{ComponentInst, RenderError};
use liyasa_core::diagnostics::{Diagnostic, Diagnostics};
use liyasa_core::document::Node;
use liyasa_core::markdown::{Audience, SiteMeta};

use crate::html::Html;
use crate::md::Markdown;

/// Renders a component's child nodes.
///
/// `liyasa-markdown` owns the real one (it owns the AST walk); the crate ships
/// [`crate::reference::Reference`] so components are testable before it lands.
pub trait Children: Sync {
    fn html(&self, nodes: &[Node], ctx: &mut HtmlCtx<'_>) -> Result<(), RenderError>;
    fn markdown(&self, nodes: &[Node], ctx: &mut MarkdownCtx<'_>) -> Result<(), RenderError>;
}

/// A child renderer that emits nothing, for a context with no AST walk.
pub struct NoChildren;

impl Children for NoChildren {
    fn html(&self, _nodes: &[Node], _ctx: &mut HtmlCtx<'_>) -> Result<(), RenderError> {
        Ok(())
    }

    fn markdown(&self, _nodes: &[Node], _ctx: &mut MarkdownCtx<'_>) -> Result<(), RenderError> {
        Ok(())
    }
}

static NO_CHILDREN: NoChildren = NoChildren;

/// What both sinks carry: where children come from, who the output is for, and
/// where diagnostics go.
pub struct Shared<'a> {
    pub(crate) children: &'a dyn Children,
    pub site: Option<&'a SiteMeta>,
    pub audience: Audience,
    /// The CSP nonce for this response; empty when the output is not a page.
    pub nonce: &'a str,
    pub diagnostics: Diagnostics,
}

impl<'a> Shared<'a> {
    pub fn new(children: &'a dyn Children) -> Self {
        Self {
            children,
            site: None,
            audience: Audience::default(),
            nonce: "",
            diagnostics: Diagnostics::new(),
        }
    }

    pub fn site(mut self, site: &'a SiteMeta) -> Self {
        self.site = Some(site);
        self
    }

    pub fn audience(mut self, audience: Audience) -> Self {
        self.audience = audience;
        self
    }

    pub fn nonce(mut self, nonce: &'a str) -> Self {
        self.nonce = nonce;
        self
    }
}

pub struct HtmlCtx<'a> {
    pub out: Html,
    pub shared: Shared<'a>,
}

impl<'a> HtmlCtx<'a> {
    pub fn new(children: &'a dyn Children) -> Self {
        Self {
            out: Html::new(),
            shared: Shared::new(children),
        }
    }

    pub fn with(shared: Shared<'a>) -> Self {
        Self {
            out: Html::new(),
            shared,
        }
    }

    /// A sink whose output goes nowhere, for the frozen adapter in `declare!`.
    pub fn detached() -> HtmlCtx<'static> {
        HtmlCtx {
            out: Html::new(),
            shared: Shared::new(&NO_CHILDREN),
        }
    }

    /// Renders `nodes` straight into the current position.
    ///
    /// The child renderer receives this context, not a bare buffer, so a
    /// nested component sees the same site, audience, and diagnostics sink as
    /// the component that contains it.
    pub fn children(&mut self, nodes: &[Node]) -> Result<(), RenderError> {
        let children = self.shared.children;
        children.html(nodes, self)
    }

    /// Renders `nodes` into a fragment, for a component that has to look at its
    /// children's markup before placing it.
    pub fn children_fragment(&mut self, nodes: &[Node]) -> Result<String, RenderError> {
        let outer = std::mem::replace(&mut self.out, Html::new());
        let result = self.children(nodes);
        let fragment = std::mem::replace(&mut self.out, outer);
        result?;
        Ok(fragment.finish())
    }

    pub fn report(&mut self, diagnostic: Diagnostic) {
        self.shared.diagnostics.push(diagnostic);
    }

    pub fn finish(self) -> String {
        self.out.finish()
    }
}

pub struct MarkdownCtx<'a> {
    pub out: Markdown,
    pub shared: Shared<'a>,
}

impl<'a> MarkdownCtx<'a> {
    pub fn new(children: &'a dyn Children) -> Self {
        Self {
            out: Markdown::new(),
            shared: Shared::new(children),
        }
    }

    pub fn with(shared: Shared<'a>) -> Self {
        Self {
            out: Markdown::new(),
            shared,
        }
    }

    pub fn detached() -> MarkdownCtx<'static> {
        MarkdownCtx {
            out: Markdown::new(),
            shared: Shared::new(&NO_CHILDREN),
        }
    }

    pub fn children(&mut self, nodes: &[Node]) -> Result<(), RenderError> {
        let children = self.shared.children;
        children.markdown(nodes, self)
    }

    /// Children rendered into their own document, so a component can indent or
    /// re-wrap them.
    pub fn children_fragment(&mut self, nodes: &[Node]) -> Result<String, RenderError> {
        let outer = std::mem::replace(&mut self.out, Markdown::new());
        let result = self.children(nodes);
        let fragment = std::mem::replace(&mut self.out, outer);
        result?;
        Ok(fragment.finish())
    }

    /// Children inside a block quote, which is how callouts serialize.
    pub fn quote_children(&mut self, nodes: &[Node]) -> Result<(), RenderError> {
        let nesting = self.out.push_quote();
        let result = self.children(nodes);
        self.out.pop(nesting);
        result
    }

    /// Children inside one list item, which is how cards and steps serialize.
    pub fn item_children(&mut self, marker: &str, nodes: &[Node]) -> Result<(), RenderError> {
        let nesting = self.out.push_item(marker);
        let result = self.children(nodes);
        self.out.pop(nesting);
        result
    }

    pub fn report(&mut self, diagnostic: Diagnostic) {
        self.shared.diagnostics.push(diagnostic);
    }

    /// An absolute URL for the Markdown output (RX-61: agent links are
    /// absolute). Relative when the site origin is not known.
    pub fn absolute(&self, url: &str) -> String {
        match self.shared.site {
            Some(site) if url.starts_with('/') => {
                format!(
                    "{}{url}",
                    site.canonical_origin.as_str().trim_end_matches('/')
                )
            }
            _ => url.to_owned(),
        }
    }

    pub fn audience(&self) -> Audience {
        self.shared.audience
    }

    pub fn finish(self) -> String {
        self.out.finish()
    }
}

/// What a component implements. The frozen `Component` methods forward here.
pub trait Render {
    fn html(&self, inst: &ComponentInst, ctx: &mut HtmlCtx<'_>) -> Result<(), RenderError>;
    fn markdown(&self, inst: &ComponentInst, ctx: &mut MarkdownCtx<'_>) -> Result<(), RenderError>;

    /// Plain text for the search index. Ctx-free, as §34.9 requires.
    fn text(&self, inst: &ComponentInst) -> String {
        crate::text::of(&inst.children)
    }

    /// Checks this component can make that the prop schema cannot express:
    /// two props that exclude each other, a URL from an unlisted provider, a
    /// missing alt text that deserves its own code.
    fn validate(&self, _inst: &ComponentInst, _out: &mut Diagnostics) {}
}
