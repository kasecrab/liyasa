//! HTML, Markdown, and plain-text serializations of the Rendered AST
//! (PRD §11.7, CM-55).

pub mod html;
pub mod markdown;
pub mod text;

use liyasa_core::components::{RenderCtx, Renderer};
use liyasa_core::document::Document;
use liyasa_core::markdown::{Audience, SiteMeta};

/// The theme renders the page (§34.9).
///
/// The `ctx` parameter is not in §34.9's declaration; it is there because
/// `Renderer::component_html` needs one and `RenderCtx` cannot be constructed
/// outside `liyasa-core`. See `plan/rfcs/0006-render-ctx-is-unconstructible.md`.
// TODO(rfc-0006): drop `ctx` once `Renderer` can hand one over.
pub fn render_html(doc: &Document, theme: &dyn Renderer, ctx: &mut RenderCtx) -> String {
    html::render(&doc.root, theme, ctx)
}

/// The Markdown an agent is served and the formatter writes (§11.7).
pub fn render_markdown(doc: &Document, audience: Audience, _site: &SiteMeta) -> String {
    markdown::render_for(&doc.root, audience)
}

/// The plain text the search index stores.
pub fn render_text(doc: &Document) -> String {
    text::render(&doc.root)
}
