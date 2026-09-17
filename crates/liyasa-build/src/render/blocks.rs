//! The bridge between the AST walk and the component registry.
//!
//! `liyasa_markdown::render::html` renders every structural node itself and
//! hands two kinds back to the caller: components and code blocks. Both are
//! rendered here through `liyasa-components`' sinks.

use liyasa_components::registry::Registry;
use liyasa_components::render::{HtmlCtx, Shared};
use liyasa_components::{Reference, fence};
use liyasa_core::build::Variant;
use liyasa_core::components::{ComponentInst, RenderError};
use liyasa_core::diagnostics::{Diagnostic, Diagnostics, code};
use liyasa_core::document::{Block, BlockKind};
use liyasa_core::markdown::SiteMeta;

/// Renders the two node kinds the AST walk delegates.
pub struct Blocks<'a> {
    registry: &'a Registry,
    site: &'a SiteMeta,
    nonce: &'a str,
    variant: Variant,
    diagnostics: Diagnostics,
}

impl<'a> Blocks<'a> {
    pub fn new(registry: &'a Registry, site: &'a SiteMeta) -> Self {
        Self {
            registry,
            site,
            nonce: "",
            variant: Variant::default(),
            diagnostics: Diagnostics::new(),
        }
    }

    /// Which variant this render is for (§6.6.3). The default admits no gated
    /// block, which is what an artefact written once per page has to mean.
    #[must_use]
    pub fn variant(mut self, variant: Variant) -> Self {
        self.variant = variant;
        self
    }

    #[must_use]
    pub fn nonce(mut self, nonce: &'a str) -> Self {
        self.nonce = nonce;
        self
    }

    pub fn take_diagnostics(&mut self) -> Diagnostics {
        std::mem::replace(&mut self.diagnostics, Diagnostics::new())
    }
}

impl liyasa_markdown::render::html::Blocks for Blocks<'_> {
    /// `children` is the walk's own rendering of the child nodes; a component
    /// renders its children itself through the sink, because most of them place
    /// the children rather than concatenating them.
    fn component(&mut self, inst: &ComponentInst, _children: &str) -> Result<String, RenderError> {
        let Some(component) = self.registry.resolve(&inst.name) else {
            return Err(RenderError::Component(inst.name.clone()));
        };
        let reference = Reference::with(self.registry);
        let shared = Shared::new(&reference)
            .site(self.site)
            .nonce(self.nonce)
            .variant(self.variant.clone());
        let mut ctx = HtmlCtx::with(shared);
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
        let shared = Shared::new(&reference)
            .site(self.site)
            .nonce(self.nonce)
            .variant(self.variant.clone());
        let mut ctx = HtmlCtx::with(shared);
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

/// A component the registry does not have, reported rather than rendered.
pub fn unknown_component(name: &str, suggestion: Option<&str>) -> Diagnostic {
    let diagnostic = Diagnostic::new(code::E0350, format!("unknown component `{name}`"));
    match suggestion {
        Some(known) => diagnostic.help(format!("did you mean `{known}`?")),
        None => diagnostic,
    }
}
