//! Liyasa's built-in components and the user component loader (PRD §9).
//!
//! One module per component under [`components`], named after its directive.
//! Every component implements the frozen [`liyasa_core::components::Component`]
//! contract and, because that contract has no usable output sink yet, the
//! crate-local [`render::Render`] trait that carries one. See
//! `plan/rfcs/0004-component-render-sinks.md`.

pub mod html;
pub mod md;

pub use liyasa_core::components::{
    Component, ComponentInst, ComponentRegistry, EditorBlock, FormField, PropDef, PropSchema,
    PropType, RenderError, SlotDef, Widget,
};
pub use liyasa_core::markdown::ComponentKind;
