//! The default theme: tokens, partials, layouts, and the reader runtime
//! (PRD §10, §11).
//!
//! Three layers, each overridable on its own (§10): [`tokens`] are CSS custom
//! properties generated from `theme.*`, partials are minijinja templates for
//! every region of the page, and the assets are the compiled stylesheet and the
//! progressive-enhancement runtime.

pub mod a11y;
pub mod actions;
pub mod banner;
pub mod color;
pub mod config;
pub mod context;
pub mod css;
pub mod fonts;
pub mod icons;
pub mod nav;
pub mod og;
pub mod outputs;
pub mod presets;
pub mod preview;
pub mod runtime;
pub mod safety;
pub mod strings;
pub mod stylesheet;
pub mod theme;
pub mod tokens;
