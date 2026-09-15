//! Liyasa Markdown (PRD §7.5, §7.16).
//!
//! The crate turns a page's bytes into the two representations of §7.16: a
//! lossless [`SourceDocument`](liyasa_core::SourceDocument) for the editor and
//! the formatter, and a Rendered AST for the build. It performs no I/O, so it
//! builds for `wasm32-unknown-unknown` (§6.2).
//!
//! The crate root is shared. WP-02 owns the file and contributes `source`;
//! WP-03 contributes `ast`, `directives`, `render`, and `sanitize`. Append to
//! it; do not rewrite it. `plan/rfcs/0004-markdown-crate-skeleton.md` records
//! how the two halves met.

pub mod source;

pub use source::{normalize, scan};
