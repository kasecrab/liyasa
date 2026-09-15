//! Liyasa's built-in components and the user component loader (PRD §9).
//!
//! One module per component under [`components`], named after its directive.
//! Every component implements the frozen [`liyasa_core::components::Component`]
//! contract and, because that contract has no usable output sink yet, the
//! crate-local [`render::Render`] trait that carries one. See
//! `plan/rfcs/0004-component-render-sinks.md`.

pub mod anchor;
pub mod components;
pub mod deps;
pub mod fence;
pub mod html;
pub mod inst;
pub mod intern;
pub mod md;
pub mod nodes;
pub mod pack;
pub mod props;
pub mod provider;
pub mod reference;
pub mod registry;
pub mod render;
pub mod schema;
pub mod text;
pub mod user;

use liyasa_core::diagnostics::Diagnostics;

/// Everything wrong with one component call: the schema check and whatever the
/// component itself adds.
pub fn validate(component: &dyn AnyComponent, inst: &ComponentInst, out: &mut Diagnostics) {
    props::validate(inst, component.schema(), out);
    Render::validate(component, inst, out);
}

pub use reference::Reference;
pub use registry::{AnyComponent, Registry};
pub use render::{Children, HtmlCtx, MarkdownCtx, Render};

pub use liyasa_core::components::{
    Component, ComponentInst, ComponentRegistry, EditorBlock, FormField, PropDef, PropSchema,
    PropType, RenderError, SlotDef, Widget,
};
pub use liyasa_core::markdown::ComponentKind;
