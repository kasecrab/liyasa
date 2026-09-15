//! Liyasa Markdown: the Source Document, the templating pass, and the
//! formatter (PRD §7.2, §7.3, §7.16).
//!
//! The crate does no I/O: `scan` and `format` take text, `expand` takes a
//! context the caller assembled, and snippets are resolved through a
//! [`source::SnippetResolver`] the caller supplies. That is what lets the same
//! code build for `wasm32-unknown-unknown` and run in the editor (§6.2).
//!
//! The types these entry points exchange are frozen in `liyasa-core`
//! (§34.9); this crate owns only their behaviour.

pub mod source;

pub use source::normalize;
