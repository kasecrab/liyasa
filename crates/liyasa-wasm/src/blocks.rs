//! The bridge between the AST walk and the component registry.
//!
//! `liyasa_markdown::render::html` renders every structural node itself and
//! hands two kinds back: components and code blocks. Both go through
//! `liyasa-components`' sinks, because `Renderer` needs a `RenderCtx` no crate
//! outside `liyasa-core` can build
//! (`plan/rfcs/0302-render-ctx-is-unconstructible.md`).
//!
//! Code fences render unhighlighted here. ED-06 keeps the grammars out of the
//! core module and the editor fetches one per language on the open page, so
//! `highlighted` is whatever the caller already had — never something this
//! crate computed.

use liyasa_components::registry::Registry;
use liyasa_components::render::{HtmlCtx, Shared};
use liyasa_components::{Reference, fence};
use liyasa_core::components::{ComponentInst, RenderError};
use liyasa_core::diagnostics::Diagnostics;
use liyasa_core::document::{Block, BlockKind};
use liyasa_core::markdown::SiteMeta;

pub struct Blocks<'a> {
    registry: &'a Registry,
    site: &'a SiteMeta,
    diagnostics: Diagnostics,
}

impl<'a> Blocks<'a> {
    pub fn new(registry: &'a Registry, site: &'a SiteMeta) -> Self {
        Self {
            registry,
            site,
            diagnostics: Diagnostics::new(),
        }
    }

    pub fn take_diagnostics(&mut self) -> Diagnostics {
        std::mem::replace(&mut self.diagnostics, Diagnostics::new())
    }
}

/// `Blocks::math` is left at its default, which declines and emits the LaTeX as
/// inert text. ED-06 puts MathML rendering in the preview endpoint, server-side,
/// rather than in the module.
impl liyasa_markdown::render::html::Blocks for Blocks<'_> {
    fn component(&mut self, inst: &ComponentInst, _children: &str) -> Result<String, RenderError> {
        let Some(component) = self.registry.resolve(&inst.name) else {
            return Err(RenderError::Component(inst.name.clone()));
        };
        let reference = Reference::with(self.registry);
        let mut ctx = HtmlCtx::with(Shared::new(&reference).site(self.site));
        component.html(inst, &mut ctx)?;
        self.diagnostics
            .extend(std::mem::take(&mut ctx.shared.diagnostics).into_vec());
        Ok(ctx.finish())
    }

    fn code(&mut self, block: &Block, body: &str) -> Result<String, RenderError> {
        let BlockKind::CodeBlock {
            lang,
            attrs,
            highlighted,
        } = &block.kind
        else {
            return Err(RenderError::Component("code".to_owned()));
        };
        let reference = Reference::with(self.registry);
        let mut ctx = HtmlCtx::with(Shared::new(&reference).site(self.site));
        fence::render_html(
            &mut ctx.out,
            lang.as_deref(),
            body,
            &fence::CodeOptions::read(attrs),
            highlighted.as_deref(),
        );
        Ok(ctx.finish())
    }
}
