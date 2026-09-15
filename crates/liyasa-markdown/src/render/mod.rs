//! HTML, Markdown, and plain-text serializations of the Rendered AST
//! (PRD §11.7, CM-55).

#[cfg(feature = "highlight")]
pub mod highlight;
pub mod html;
pub mod markdown;
pub mod text;

use liyasa_core::components::{ComponentInst, RenderCtx, RenderError, Renderer};
use liyasa_core::document::{Block, Document};
use liyasa_core::markdown::{Audience, SiteMeta};

/// The theme renders the page (§34.9).
///
/// The `ctx` parameter is not in §34.9's declaration; it is there because
/// `Renderer::component_html` needs one and `RenderCtx` cannot be constructed
/// outside `liyasa-core`. See `plan/rfcs/0302-render-ctx-is-unconstructible.md`.
// TODO(rfc-0302): drop `ctx` once `Renderer` can hand one over.
pub fn render_html(doc: &Document, theme: &dyn Renderer, ctx: &mut RenderCtx) -> String {
    html::render(&doc.root, &mut Themed { theme, ctx })
}

/// Closes `Renderer` over the caller's context so the renderer itself never
/// touches a type it cannot build.
struct Themed<'a> {
    theme: &'a dyn Renderer,
    ctx: &'a mut RenderCtx,
}

impl html::Blocks for Themed<'_> {
    fn component(&mut self, inst: &ComponentInst, children: &str) -> Result<String, RenderError> {
        self.theme.component_html(inst, children, self.ctx)
    }

    fn code(&mut self, block: &Block, body: &str) -> Result<String, RenderError> {
        self.theme.code_block(block, body, self.ctx)
    }
}

/// The Markdown an agent is served and the formatter writes (§11.7).
pub fn render_markdown(doc: &Document, audience: Audience, _site: &SiteMeta) -> String {
    markdown::render_for(&doc.root, audience)
}

/// The plain text the search index stores.
pub fn render_text(doc: &Document) -> String {
    text::render(&doc.root)
}
