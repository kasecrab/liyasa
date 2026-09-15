//! The default theme: tokens, partials, layouts, and the reader runtime
//! (PRD §10, §11).
//!
//! Three layers, each overridable on its own (§10): [`tokens`] are CSS custom
//! properties generated from `theme.*`, partials are minijinja templates for
//! every region of the page, and the assets are the compiled stylesheet and the
//! progressive-enhancement runtime.

pub mod color;
pub mod config;
pub mod tokens;
